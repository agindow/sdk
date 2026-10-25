//! The bounded tee — a copy of the stream for inspection, off the client path.
//!
//! TI-06, contract §3.1. A tee wraps the upstream body and forwards every read
//! unmodified; the copy it keeps is capped, so a 1GB response retains at most
//! `cap` bytes and no re-encoding ever happens. Parsing is not here:
//! [`Read::read`] only copies, and [`Tee::into_inspection`] parses after the
//! stream has been driven — a slow parse cannot delay a byte.
//!
//! **One cost a caller should know about.** Rust 1.99 replaced `Read::size_hint`
//! with `core::io::size_hint::SizeHint`, and sealed it behind a blanket impl —
//! so no `Read` wrapper can forward the hint, and [`read_to_end`] cannot size
//! its buffer from a known-length body. Measured: a 1 MiB body allocates 2 MiB
//! through a tee where a bare `Cursor` allocates exactly 1 MiB. The wire path
//! does not pay this — it polls frames into a fixed buffer — but anything that
//! drains a tee with `read_to_end` should pre-reserve. It is a property of
//! `Read` wrappers in general; a trivial pass-through measures the same.
//!
//! [`read_to_end`]: std::io::Read::read_to_end

use std::io::{self, Read};

use crate::{Inspection, Precision, inspect};

#[cfg(test)]
mod allocation_tests;

/// The head retained per response unless [`Tee::with_cap`] says otherwise.
pub const DEFAULT_CAP: usize = 8 * 1024 * 1024;

/// TI-37's 64 KiB forwarding frames: the tee measured 776.4 µs for
/// 32 MiB versus 2.003 ms doubling and 2.266–2.395 ms in coarse steps (local
/// Criterion means; TI-37 records the same-invocation comparison). Grow small
/// heads on demand; crossing this commits the cap, deliberately trading memory
/// for latency. See contract §10.8.
const SMALL_HEAD_LIMIT: usize = 64 * 1024;

/// Forwards bytes unchanged while retaining a copy of their head.
///
/// [`Tee::new`] caps the copy at [`DEFAULT_CAP`] (8 MiB); a longer stream is
/// not stored beyond the cap — the bytes past it still reach the client, and
/// [`Tee::truncated`] records that the copy is a prefix. An upstream that dies
/// mid-body marks it too: the copy is then just as much a prefix, and a
/// classifier must not read the short head as a finished document.
pub struct Tee<R> {
    inner: R,
    cap: usize,
    captured: Vec<u8>,
    truncated: bool,
}

impl<R> Tee<R> {
    pub fn new(inner: R) -> Self {
        Self::with_cap(inner, DEFAULT_CAP)
    }

    pub fn with_cap(inner: R, cap: usize) -> Self {
        Self {
            inner,
            cap,
            captured: Vec::new(),
            truncated: false,
        }
    }

    /// The cap in force.
    pub fn cap(&self) -> usize {
        self.cap
    }

    /// The retained head, borrowed — inspection never needs a second copy.
    pub fn captured(&self) -> &[u8] {
        &self.captured
    }

    /// True when the retained head is a *prefix of an unfinished body*.
    ///
    /// Two things make that true, and the classifier cannot tell them apart —
    /// nor should it: the stream exceeded [`Tee::cap`], or it died before EOF.
    /// Either way the bytes held are not the whole body, so [`Tee::captured`]
    /// must not be read as one.
    pub fn truncated(&self) -> bool {
        self.truncated
    }

    /// Parse the retained head into the classification input.
    ///
    /// Consuming, so the parse runs off the client path: the caller drives the
    /// body to EOF first, then inspects.
    pub fn into_inspection(self) -> Inspection {
        inspect(&self.captured, self.truncated, None)
    }

    /// Inspect after EOF with an explicit whole-document precision decision.
    pub fn into_inspection_with_precision(self, precision: Precision) -> Inspection {
        inspect(&self.captured, self.truncated, Some(precision))
    }

    fn capture(&mut self, bytes: &[u8]) {
        if self.truncated || bytes.is_empty() {
            return;
        }
        let room = self.cap - self.captured.len();
        let needed = self.captured.len() + bytes.len().min(room);
        if needed > self.captured.capacity() {
            let target = if needed <= SMALL_HEAD_LIMIT {
                needed.next_power_of_two().min(self.cap)
            } else {
                self.cap
            };
            // Exact reservation prevents Vec's implicit doubling from
            // exceeding a custom cap. After the small head there is only one
            // growth, even when a reader delivers the stream byte by byte.
            self.captured.reserve_exact(target - self.captured.len());
        }
        if bytes.len() > room {
            // More arrived than fits: keep the head, remember the rest exists,
            // and copy nothing further — memory stays bounded by the cap.
            self.captured.extend_from_slice(&bytes[..room]);
            self.truncated = true;
        } else {
            self.captured.extend_from_slice(bytes);
        }
    }
}

impl<R: Read> Read for Tee<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        // One read, one forward, one bounded copy. Nothing here parses and
        // nothing here buffers beyond `cap`, so a client's read never waits on
        // work it did not ask for.
        match self.inner.read(buf) {
            Ok(n) => {
                self.capture(&buf[..n]);
                Ok(n)
            }
            Err(error) => {
                // The stream stopped before EOF, so the head is a prefix of an
                // unfinished body — the same fact the cap records, reached by
                // the network instead of by us. `Interrupted` is the exception:
                // it asks to be retried rather than reporting a dead stream, so
                // the body may still finish (`read_to_end` and hyper both loop
                // on it). The error itself always reaches the client untouched.
                if error.kind() != io::ErrorKind::Interrupted {
                    self.truncated = true;
                }
                Err(error)
            }
        }
    }
}
