//! Shared native readiness and cooldown, without locks or waiting (TI-42).
use std::panic::{RefUnwindSafe, UnwindSafe};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use std::time::Duration;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;

use crate::availability::{Budget, EngineFault, Guarded, RegistrySnapshot, Status, contain};
use crate::config::CallContext;

/// Monotonic elapsed time; injectable so recovery tests never sleep.
pub trait Clock: RefUnwindSafe {
    fn now(&self) -> Duration;
}

#[cfg(not(target_arch = "wasm32"))]
pub struct MonotonicClock(Instant);
#[cfg(target_arch = "wasm32")]
pub struct MonotonicClock;
#[cfg(target_arch = "wasm32")]
std::thread_local! { static HOST_TIME: std::cell::Cell<Duration> = const { std::cell::Cell::new(Duration::ZERO) }; }
/// Embedded hosts supply monotonic elapsed time before guest entry. Values can
/// only advance; readiness never reads an unavailable guest platform clock.
#[cfg(target_arch = "wasm32")]
pub fn advance_host_time(elapsed: Duration) {
    HOST_TIME.with(|time| time.set(time.get().max(elapsed)));
}
impl Clock for MonotonicClock {
    fn now(&self) -> Duration {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.0.elapsed()
        }
        #[cfg(target_arch = "wasm32")]
        {
            HOST_TIME.with(std::cell::Cell::get)
        }
    }
}

// 0..=2 are closed-circuit consecutive failures. Other values encode a
// cooldown deadline plus 3. MAX reserves an exclusive probe slot.
const PROBING: u64 = u64::MAX;
const COOLDOWN_NS: u64 = 1_000_000_000;

struct Shared<C> {
    ready: AtomicBool,
    state: AtomicU64,
    clock: C,
}

/// Attach one gate to one engine and share clones across callers. Readiness is
/// a local control-plane input. Inspection and original operations run unlocked.
pub struct AvailabilityGate<C = MonotonicClock>(Arc<Shared<C>>);

impl<C> Clone for AvailabilityGate<C> {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

impl AvailabilityGate {
    pub fn new(ready: bool) -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        let clock = MonotonicClock(Instant::now());
        #[cfg(target_arch = "wasm32")]
        let clock = MonotonicClock;
        Self::with_clock(ready, clock)
    }
}

impl<C: Clock> AvailabilityGate<C> {
    pub fn with_clock(ready: bool, clock: C) -> Self {
        Self(Arc::new(Shared {
            ready: AtomicBool::new(ready),
            state: AtomicU64::new(0),
            clock,
        }))
    }

    /// Admit once per call; successful request work must not reset failures
    /// from prior response phases. The permit completes after both phases.
    pub(crate) fn admit(&self) -> Result<Evaluation<C>, EngineFault> {
        Ok(Evaluation {
            gate: self.clone(),
            probe: self.enter()?,
            status: Status::Inspected,
        })
    }

    pub fn set_ready(&self, ready: bool) {
        self.0.ready.store(ready, Ordering::Relaxed);
    }

    fn now(&self) -> u64 {
        // Saturation keeps the sentinel reserved even for an injected clock.
        u64::try_from(self.0.clock.now().as_nanos())
            .unwrap_or(PROBING - COOLDOWN_NS - 4)
            .min(PROBING - COOLDOWN_NS - 4)
    }

    fn deadline(&self) -> u64 {
        self.now() + COOLDOWN_NS + 3
    }

    /// `true` owns the half-open probe; `false` is an ordinary evaluation.
    fn enter(&self) -> Result<bool, EngineFault> {
        if !self.0.ready.load(Ordering::Relaxed) {
            return Err(EngineFault::NotReady);
        }
        let state = self.0.state.load(Ordering::Relaxed);
        if state <= 2 {
            return Ok(false);
        }
        if state == PROBING || self.now() < state - 3 {
            return Err(EngineFault::CircuitOpen);
        }
        self.0
            .state
            .compare_exchange(state, PROBING, Ordering::Relaxed, Ordering::Relaxed)
            .map(|_| true)
            .map_err(|_| EngineFault::CircuitOpen)
    }

    fn complete(&self, probe: bool, status: Status) {
        let failed = matches!(status, Status::Bypassed(_));
        if probe {
            self.0
                .state
                .store(if failed { self.deadline() } else { 0 }, Ordering::Relaxed);
        } else {
            // Completion order defines consecutive failures. A late completion
            // from before opening cannot change a cooldown or probe slot.
            let mut state = self.0.state.load(Ordering::Relaxed);
            while state <= 2 {
                let next = if !failed {
                    0
                } else if state < 2 {
                    state + 1
                } else {
                    self.deadline()
                };
                match self.0.state.compare_exchange_weak(
                    state,
                    next,
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => break,
                    Err(current) => state = current,
                }
            }
        }
    }
}

/// Owns call-level breaker accounting, including cleanup on upstream unwind.
pub(crate) struct Evaluation<C: Clock> {
    gate: AvailabilityGate<C>,
    probe: bool,
    status: Status,
}
impl<C: Clock> Evaluation<C> {
    pub(crate) fn evaluate<F, O>(&mut self, work: F) -> Result<O, EngineFault>
    where
        F: FnOnce() -> Result<O, EngineFault> + UnwindSafe,
    {
        let result = contain(work);
        if let Err(fault) = &result {
            self.status = Status::Bypassed(*fault)
        }
        result
    }
}
impl<C: Clock> Drop for Evaluation<C> {
    fn drop(&mut self) {
        self.gate.complete(self.probe, self.status)
    }
}

impl CallContext {
    /// Readiness/circuit admission around TI-40 containment. Every bypass keeps
    /// the original operation outside recovery and invokes it exactly once.
    pub fn guard_with<I, R, F, C, O, T>(
        &self,
        gate: &AvailabilityGate<T>,
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
        T: Clock,
    {
        let status = match gate.enter() {
            Err(fault) => {
                // An unused closure can own fallible engine resources. Dispose
                // before caller code so its Drop cannot replace that outcome
                // or double-panic while a caller panic is unwinding.
                let cleaned = contain(move || {
                    drop(inspect);
                    Ok(())
                });
                Status::Bypassed(cleaned.err().unwrap_or(fault))
            }
            Ok(probe) => {
                let evaluated = self.guard(input, registry, limit, inspect, || ());
                gate.complete(probe, evaluated.status);
                evaluated.status
            }
        };
        Guarded {
            status,
            outcome: operation(),
        }
    }
}
