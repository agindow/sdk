//! TI-37 comparators: keep the old policy and candidate coarse steps in the
//! same Criterion run, so baseline drift cannot masquerade as an improvement.

use agw_inspector::DEFAULT_CAP;
use std::io::{self, Read};

// Same frame-sized threshold as Tee: TI-37 measures the policies below with
// 64 KiB reads; the 862-byte fixture stays on demand-sized allocations.
pub const SMALL: usize = 64 * 1024;
pub const MIB: usize = 1024 * 1024;

pub struct Capture<R> {
    inner: R,
    head: Vec<u8>,
    step: Option<usize>,
    truncated: bool,
}

impl<R> Capture<R> {
    pub fn new(inner: R, step: Option<usize>) -> Self {
        Self {
            inner,
            head: Vec::new(),
            step,
            truncated: false,
        }
    }

    pub fn captured(&self) -> &[u8] {
        &self.head
    }
}

impl<R: Read> Read for Capture<R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buffer)?;
        if self.truncated || n == 0 {
            return Ok(n);
        }
        let keep = n.min(DEFAULT_CAP - self.head.len());
        let needed = self.head.len() + keep;
        if let Some(step) = self.step
            && needed > self.head.capacity()
        {
            let target = if needed <= SMALL {
                needed
                    .max(self.head.capacity().saturating_mul(2))
                    .min(SMALL)
            } else {
                needed.div_ceil(step).saturating_mul(step).min(DEFAULT_CAP)
            };
            self.head.reserve_exact(target - self.head.len());
        }
        self.head.extend_from_slice(&buffer[..keep]);
        self.truncated = n > keep;
        Ok(n)
    }
}
