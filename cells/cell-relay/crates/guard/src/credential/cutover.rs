//! Host-driven credential cutover. Bindings are opaque handles. A dispatch
//! clones one Arc before calling upstream, fencing in-flight work across revert.
use super::{Cause, Evidence, health::SetupError};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
mod restart;
mod types;
pub use types::{Config, Definition, Kind, Phase, Probe, Receipt, UnsupportedReason};
pub struct Cutover<B> {
    active: Arc<B>,
    candidate: Arc<B>,
    selected: Arc<AtomicBool>,
    config: Config,
    receipt: Receipt,
    started: Option<u64>,
    last_tick: u64,
    successes: usize,
    plan: Vec<Definition>,
    passed: Vec<bool>,
}
/// Detached production selection: never locks the operator state machine.
pub struct Dispatcher<B> {
    active: Arc<B>,
    candidate: Arc<B>,
    selected: Arc<AtomicBool>,
}
impl<B> Dispatcher<B> {
    pub fn dispatch(&self) -> Arc<B> {
        Arc::clone(if self.selected.load(Ordering::Acquire) {
            &self.candidate
        } else {
            &self.active
        })
    }
}
pub(super) fn reference(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 256
        && !s.chars().any(char::is_control)
        && !["sk_live_", "Bearer ", "secret=", "Authorization"]
            .iter()
            .any(|p| s.contains(p))
}
fn credential_failure(p: &Probe) -> bool {
    p.cause.is_some_and(Cause::failure) || (p.cause != Some(Cause::Quota) && p.status == 401)
}
fn successful_probe(p: &Probe) -> bool {
    p.cause.is_none() && (200..300).contains(&p.status)
}
fn matches_definition(p: &Probe, d: &Definition) -> bool {
    d.operation == p.operation
        && d.kind == p.kind
        && d.write_opt_in == p.write_opt_in
        && d.idempotency_key_present == p.idempotency_key_present
}
impl<B> Cutover<B> {
    pub fn new(
        id: &str,
        active: Arc<B>,
        candidate: Arc<B>,
        config: Config,
    ) -> Result<Self, SetupError> {
        if !reference(id)
            || !(1..=1024).contains(&config.max_probes)
            || config.deadline == 0
            || config.overlap_ms == 0
            || config.threshold == 0
        {
            return Err(SetupError);
        }
        Ok(Self {
            active,
            candidate,
            selected: Arc::new(AtomicBool::new(false)),
            config,
            started: None,
            last_tick: 0,
            successes: 0,
            plan: Vec::new(),
            passed: Vec::new(),
            receipt: Receipt {
                unsupported_reason: None,
                id: id.into(),
                phase: Phase::Idle,
                probes: Vec::new(),
                window_start: None,
                window_end: None,
                failing_operation: None,
                audit_ref: None,
                failure_count: 0,
                canary: true,
            },
        })
    }
    pub fn phase(&self) -> Phase {
        self.receipt.phase
    }
    pub fn receipt(&self) -> &Receipt {
        &self.receipt
    }
    /// Capture before executing: no retries or replay are performed by this library.
    pub fn dispatcher(&self) -> Dispatcher<B> {
        Dispatcher {
            active: Arc::clone(&self.active),
            candidate: Arc::clone(&self.candidate),
            selected: Arc::clone(&self.selected),
        }
    }
    pub fn dispatch(&self) -> Arc<B> {
        Arc::clone(if self.selected.load(Ordering::Acquire) {
            &self.candidate
        } else {
            &self.active
        })
    }
    /// Freeze the bounded plan before any candidate call. Reconfiguration of a
    /// running/aborted ID is forbidden; a retry requires a fresh cutover ID.
    pub fn set_plan(&mut self, plan: Vec<Definition>) -> bool {
        if self.phase() != Phase::Idle
            || plan.is_empty()
            || plan.len() > self.config.max_probes
            || plan.iter().any(|p| {
                !reference(&p.operation)
                    || (p.kind != Kind::Read && !(p.write_opt_in && p.idempotency_key_present))
            })
            || plan
                .iter()
                .enumerate()
                .any(|(i, p)| plan[..i].iter().any(|q| q.operation == p.operation))
        {
            return false;
        }
        self.passed = vec![false; plan.len()];
        self.plan = plan;
        true
    }
    pub fn start(&mut self, now: u64) {
        if self.phase() != Phase::Idle {
            return;
        }
        if self.plan.is_empty() {
            self.receipt.phase = Phase::Aborted;
            return;
        }
        self.receipt.phase = if self.config.dual_key == Some(true) {
            self.started = Some(now);
            Phase::Canary
        } else {
            self.receipt.unsupported_reason = Some(if self.config.dual_key == Some(false) {
                UnsupportedReason::AtomicInvalidation
            } else {
                UnsupportedReason::UnknownCapability
            });
            Phase::Unsupported
        };
    }
    /// Must be called before any upstream probe. Writes/unknown require a
    /// per-probe opt-in and host-carried idempotency key.
    pub fn admit(&self, p: &Probe) -> bool {
        self.phase() == Phase::Canary
            && self.receipt.probes.len() < self.config.max_probes
            && reference(&p.operation)
            && reference(&p.audit_ref)
            && self.plan.iter().any(|d| matches_definition(p, d))
            && (p.kind == Kind::Read || (p.write_opt_in && p.idempotency_key_present))
    }
    pub fn candidate(&self, p: &Probe) -> Option<Arc<B>> {
        self.admit(p).then(|| Arc::clone(&self.candidate))
    }
    /// Execute an admitted candidate call outside every statistical/guard session.
    /// Host timeouts remain host-owned; faults cannot affect production dispatch.
    pub fn run_probe<F>(&mut self, definition: &Probe, now: u64, execute: F) -> bool
    where
        B: std::panic::RefUnwindSafe,
        F: FnOnce(Arc<B>) -> Result<Probe, ()> + std::panic::UnwindSafe,
    {
        let Some(binding) = self.candidate(definition) else {
            self.abort();
            return false;
        };
        let result = crate::availability::contain(move || {
            execute(binding).map_err(|()| crate::availability::EngineFault::Internal)
        });
        match result {
            Ok(probe) if probe.operation == definition.operation => self.record_probe(probe, now),
            _ => {
                self.abort();
                false
            }
        }
    }
    pub fn record_probe(&mut self, p: Probe, now: u64) -> bool {
        if !self.admit(&p)
            || self
                .started
                .is_none_or(|start| now.saturating_sub(start) >= self.config.deadline)
            || !(100..=599).contains(&p.status)
        {
            self.abort();
            return false;
        }
        let failed = credential_failure(&p);
        if successful_probe(&p) {
            self.successes += 1;
            if let Some(index) = self.plan.iter().position(|d| d.operation == p.operation)
                && let Some(passed) = self.passed.get_mut(index)
            {
                *passed = true;
            }
        }
        self.receipt.probes.push(p);
        if failed
            || (self.receipt.probes.len() >= self.config.max_probes
                && self.passed.iter().any(|passed| !passed))
        {
            self.abort();
        }
        self.phase() == Phase::Canary
    }
    /// Explicit host assertion that its entire declared probe plan completed.
    pub fn promote(&mut self, now: u64) -> bool {
        if self.phase() != Phase::Canary
            || self.passed.is_empty()
            || self.passed.iter().any(|passed| !passed)
            || self
                .started
                .is_none_or(|start| now.saturating_sub(start) >= self.config.deadline)
        {
            return false;
        }
        let Some(end) = now.checked_add(self.config.overlap_ms) else {
            self.abort();
            return false;
        };
        self.receipt.phase = Phase::Overlap;
        self.selected.store(true, Ordering::Release);
        self.receipt.window_start = Some(now);
        self.receipt.window_end = Some(end);
        true
    }
    pub fn abort(&mut self) {
        if self.phase() == Phase::Canary {
            self.receipt.phase = Phase::Aborted;
        }
    }
    pub fn revert(&mut self) {
        if matches!(self.phase(), Phase::Overlap | Phase::Promoted) {
            self.selected.store(false, Ordering::Release);
            self.receipt.phase = Phase::Reverted;
        }
    }
    pub fn failure(
        &mut self,
        cause: Cause,
        evidence: Evidence,
        operation: &str,
        audit_ref: &str,
        _now: u64,
    ) {
        if self.phase() != Phase::Overlap {
            return;
        }
        if cause == Cause::Recovered {
            self.receipt.failure_count = 0;
            return;
        }
        if !cause.failure() {
            return;
        }
        self.receipt.failure_count = self.receipt.failure_count.saturating_add(1);
        if cause.terminal(evidence) || self.receipt.failure_count >= self.config.threshold {
            self.receipt.failing_operation = reference(operation).then(|| operation.into());
            self.receipt.audit_ref = reference(audit_ref).then(|| audit_ref.into());
            self.revert();
        }
    }
    /// Returns the exactly-once local release signal. Host persists the snapshot
    /// before releasing its binding. No timer, callback or I/O runs here.
    pub fn tick(&mut self, now: u64) -> bool {
        if now < self.last_tick {
            return false;
        }
        self.last_tick = now;
        if self.phase() == Phase::Canary
            && self
                .started
                .is_some_and(|start| now.saturating_sub(start) >= self.config.deadline)
        {
            self.abort();
        }
        if self.phase() == Phase::Overlap && self.receipt.window_end.is_some_and(|end| now >= end) {
            self.receipt.phase = Phase::Retired;
            return true;
        }
        false
    }
    pub fn tick_with_release<F: FnOnce()>(&mut self, now: u64, release: F) -> bool {
        let retired = self.tick(now);
        if retired {
            release();
        }
        retired
    }
}
