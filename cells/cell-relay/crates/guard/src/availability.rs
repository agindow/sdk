//! Fault containment for one inspection, independent of mode (TI-40, §10.9).
//!
//! The original operation runs outside recovery. Engines borrow immutable input
//! and a local snapshot; callers must charge the cooperative budget before work.
//! No registry I/O, policy decisions or circuit scheduling happens here.

use std::panic::{RefUnwindSafe, UnwindSafe, catch_unwind};

use crate::config::CallContext;

/// Availability failures only; policy rejection is never an engine fault.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineFault {
    NotReady,
    CircuitOpen,
    Internal,
    RegistryCorrupt,
    RegistryUnavailable,
    ResourceExhausted,
    Panic,
}

/// Prepared outside the request path. A loader reports corruption/unavailability
/// here instead of making inspection wait for a remote refresh.
#[derive(Debug, Clone, Copy)]
pub enum RegistrySnapshot<'a, R: ?Sized> {
    Ready(&'a R),
    Corrupt,
    Unavailable,
}

/// A cooperative per-call byte/work allowance, separate from the tee's cap.
/// This cannot preempt an engine or catch actual process-wide allocator OOM.
#[derive(Debug)]
pub struct Budget {
    remaining: usize,
    exhausted: bool,
}

impl Budget {
    pub fn new(limit: usize) -> Self {
        Self {
            remaining: limit,
            exhausted: false,
        }
    }

    pub fn remaining(&self) -> usize {
        self.remaining
    }

    /// Charge before allocating/inspecting. An overrun permanently faults this
    /// call, even if the engine ignores the error or attempts a smaller charge.
    pub fn charge(&mut self, units: usize) -> Result<(), EngineFault> {
        if !self.exhausted
            && let Some(remaining) = self.remaining.checked_sub(units)
        {
            self.remaining = remaining;
            return Ok(());
        }
        self.exhausted = true;
        Err(EngineFault::ResourceExhausted)
    }
}

/// Inspection completion is not evidence of a drift/policy verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Inspected,
    Bypassed(EngineFault),
}

/// Carries the caller's outcome without cloning or interpreting it. `O` may
/// itself be a vendor `Result`, including its original error object.
#[derive(Debug)]
pub struct Guarded<O> {
    pub status: Status,
    pub outcome: O,
}

/// Contain both engine work and skipped-engine cleanup with identical payload
/// disposal: dropping a caught panic payload is itself arbitrary engine code.
pub(crate) fn contain<F, T>(work: F) -> Result<T, EngineFault>
where
    F: FnOnce() -> Result<T, EngineFault> + UnwindSafe,
{
    match catch_unwind(work) {
        Ok(result) => result,
        Err(payload) => {
            // Forget only a secondary payload: its destructor cannot safely
            // be retried indefinitely if disposal raises yet another panic.
            if let Err(secondary) = catch_unwind(std::panic::AssertUnwindSafe(|| drop(payload))) {
                std::mem::forget(secondary);
            }
            Err(EngineFault::Panic)
        }
    }
}

impl CallContext {
    /// Evaluate once, then run the original operation exactly once in all modes.
    /// Only unwind-safe engine state may cross this native recovery boundary.
    /// A panic in `operation` propagates unchanged. Native aborts, double panics
    /// during unwinding and aborting hooks are outside this boundary's power.
    /// Aborting WASM guests require the host-side boundary in `cell-relay/wasm`.
    pub fn guard<I, R, F, C, O>(
        &self,
        input: &I,
        registry: RegistrySnapshot<'_, R>,
        limit: usize,
        inspect: F,
        operation: C,
    ) -> Guarded<O>
    where
        I: RefUnwindSafe + ?Sized,
        R: RefUnwindSafe + ?Sized,
        F: FnOnce(&I, &R, &mut Budget) -> Result<(), EngineFault> + UnwindSafe,
        C: FnOnce() -> O,
    {
        let evaluated = contain(move || {
            let snapshot = match registry {
                RegistrySnapshot::Ready(snapshot) => snapshot,
                RegistrySnapshot::Corrupt => return Err(EngineFault::RegistryCorrupt),
                RegistrySnapshot::Unavailable => return Err(EngineFault::RegistryUnavailable),
            };
            // Budget is created inside recovery and discarded after inspection;
            // no partially-mutated budget escapes an unwinding engine.
            let mut budget = Budget::new(limit);
            let result = inspect(input, snapshot, &mut budget);
            if budget.exhausted {
                Err(EngineFault::ResourceExhausted)
            } else {
                result
            }
        });
        let status = match evaluated {
            Ok(()) => Status::Inspected,
            Err(fault) => Status::Bypassed(fault),
        };
        Guarded {
            status,
            outcome: operation(),
        }
    }
}
