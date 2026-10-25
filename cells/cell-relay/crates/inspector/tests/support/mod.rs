//! Test doubles shared by this crate's integration suites (AGENTS.md §6).
//!
//! They live in a subdirectory rather than beside the suites because cargo
//! compiles every `tests/*.rs` as its own test binary — a helper at the top
//! level would build and run as an empty suite.

#![allow(dead_code)]

use std::cell::Cell;
use std::io::{self, ErrorKind, Read};
use std::rc::Rc;

/// A `Read` that yields `data` in fixed-size chunks.
///
/// Real sockets hand over partial reads; a tee exercised only through `Cursor`
/// would never see a capture boundary land inside one read.
pub struct ChunkedReader<'a> {
    data: &'a [u8],
    offset: usize,
    chunk: usize,
}

impl<'a> ChunkedReader<'a> {
    pub fn new(data: &'a [u8], chunk: usize) -> Self {
        Self {
            data,
            offset: 0,
            chunk,
        }
    }
}

impl Read for ChunkedReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let remaining = self.data.get(self.offset..).unwrap_or(&[]);
        let n = remaining.len().min(buf.len()).min(self.chunk);
        buf[..n].copy_from_slice(&remaining[..n]);
        self.offset += n;
        Ok(n)
    }
}

/// A `Read` that serves bytes until `after`, then fails — an upstream that
/// drops mid-response. The tee must pass that failure on unmodified.
pub struct FailingReader {
    served: usize,
    after: usize,
}

impl FailingReader {
    pub const fn after(after: usize) -> Self {
        Self { served: 0, after }
    }
}

impl Read for FailingReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.served >= self.after {
            return Err(io::Error::new(
                ErrorKind::ConnectionReset,
                "upstream dropped",
            ));
        }
        let n = buf.len().min(self.after - self.served).min(4);
        buf[..n].fill(b'u');
        self.served += n;
        Ok(n)
    }
}

/// A `Read` that fails with `Interrupted` every `every`-th read, then
/// finishes normally. A retryable error, unlike [`FailingReader`]: the body
/// still arrives in full.
pub struct InterruptingReader<'a> {
    data: &'a [u8],
    offset: usize,
    reads: usize,
    every: usize,
}

impl<'a> InterruptingReader<'a> {
    pub fn new(data: &'a [u8], every: usize) -> Self {
        Self {
            data,
            offset: 0,
            reads: 0,
            every,
        }
    }
}

impl Read for InterruptingReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.reads += 1;
        if self.reads.is_multiple_of(self.every) {
            return Err(io::Error::new(ErrorKind::Interrupted, "try again"));
        }
        let remaining = self.data.get(self.offset..).unwrap_or(&[]);
        let n = remaining.len().min(buf.len());
        buf[..n].copy_from_slice(&remaining[..n]);
        self.offset += n;
        Ok(n)
    }
}

/// Reads a tee (or any reader) to EOF into a `Vec`.
pub fn read_all<R: Read>(reader: &mut R) -> io::Result<Vec<u8>> {
    let mut out = Vec::new();
    reader.read_to_end(&mut out)?;
    Ok(out)
}

/// A `Read` that reports how it was polled: the number of reads, and the
/// largest buffer it was ever asked for.
///
/// The buffer size is the load-bearing half. The tee's whole claim is that
/// inspection never delays the response (TI-06), and a tee that read ahead
/// would do exactly that — it would block the client's next read waiting for
/// bytes the client never asked for.
pub struct SpyReader<R> {
    pub inner: R,
    pub reads: Rc<Cell<usize>>,
    pub max_requested: Rc<Cell<usize>>,
}

impl<R: Read> Read for SpyReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.reads.set(self.reads.get() + 1);
        if buf.len() > self.max_requested.get() {
            self.max_requested.set(buf.len());
        }
        self.inner.read(buf)
    }
}
