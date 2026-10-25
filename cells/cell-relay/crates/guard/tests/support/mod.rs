//! Test doubles shared by this crate's integration suites (AGENTS.md §6).
//!
//! They live in a subdirectory rather than beside the suites because cargo
//! compiles every `tests/*.rs` as its own test binary — a helper at the top
//! level would build and run as an empty suite. A subdirectory is only
//! compiled when a suite opts in with `mod support;`.
//!
//! Each binary compiles this module separately, so a double that only one
//! suite uses is dead code in the other. That is the cost of sharing here.

#![allow(dead_code)]

use std::cell::Cell;
use std::io::{self, Read};
use std::rc::Rc;

/// A `Read` that reports how it was polled: the number of reads, and the
/// largest buffer it was ever asked for.
///
/// The buffer size is the load-bearing half — it is how the forwarding path's
/// no-read-ahead property is asserted. A proxy that slurped into an internal
/// buffer would ask upstream for more than the client did (TI-05).
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
