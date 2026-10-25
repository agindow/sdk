//! Explicit native statistical execution (TI-46, runtime contract §10.27).
//! The host supplies bounded captures and local selections. Upstream executes
//! outside recovery; this API neither buffers a stream nor retries a write.
mod audit;
mod detectors;
mod evaluate;
mod plan;
mod session;
use crate::{
    availability::EngineFault,
    config::{CallContext, Mode},
    malformed::Disposition,
    readiness::AvailabilityGate,
};
pub use agw_policy::{Decision, Operation};
use agw_stats::router::{Admission, Outbox};
pub use audit::{Applied, METADATA_KEY, compose, compose_with};
pub use detectors::{Builtins, Detector, Detectors, Evidence, Record};
pub use plan::{Engine, Field, Identity, InvalidPlan, Materiality, Plan};
pub use session::Session;
use std::sync::Arc;

#[derive(Clone, Copy)]
pub struct Body<'a> {
    pub bytes: &'a [u8],
    pub truncated: bool,
    pub at: chrono::DateTime<chrono::Utc>,
    /// Optional complete TI-11 findings supplied by the local shape pipeline.
    /// Side must match this phase; skipped reports remain unavailable.
    pub findings: Option<&'a agw_diff::Report>,
}
impl<'a> Body<'a> {
    pub fn complete(bytes: &'a [u8], at: chrono::DateTime<chrono::Utc>) -> Self {
        Self {
            bytes,
            truncated: false,
            at,
            findings: None,
        }
    }
}
#[derive(Debug)]
pub struct Phase {
    pub cross_endpoint: Vec<crate::cross_endpoint::Comparison>,
    pub cross_observations: Vec<agw_invariants::cross_endpoint::Observation>,
    pub cross_scope: Option<agw_invariants::cross_endpoint::Scope>,
    pub invariants: Vec<crate::invariant::Record>,
    pub unavailable_invariants: usize,
    pub invariant_audit: Option<agw_audit::contracts::audit_artifact::AuditArtifact>,
    pub headers: crate::proxy::Headers,
    pub warning: Option<&'static str>,
    pub decision: Decision,
    pub report: agw_diff::Report,
    pub records: Vec<Record>,
    pub fault: Option<EngineFault>,
    pub malformed: Option<Disposition>,
    pub investigation: Admission,
}
impl Phase {
    fn clean() -> Self {
        Self {
            cross_endpoint: Vec::new(),
            cross_observations: Vec::new(),
            cross_scope: None,
            invariants: Vec::new(),
            unavailable_invariants: 0,
            invariant_audit: None,
            headers: crate::proxy::Headers::new(),
            warning: None,
            decision: Decision::Pass,
            report: agw_diff::Report::default(),
            records: Vec::new(),
            fault: None,
            malformed: None,
            investigation: Admission::NotNeeded,
        }
    }
    fn bypass(fault: EngineFault) -> Self {
        Self {
            fault: Some(fault),
            ..Self::clean()
        }
    }
}
/// A response halt reports the completed attempt rather than erasing it.
/// No repair bytes, retry, rollback or write-integrity claims exist in this API.
#[must_use = "Inspect response policy: a completed write cannot be undone by a response halt"]
#[derive(Debug)]
pub struct Outcome<O> {
    pub mode: Mode,
    pub request: Phase,
    pub response: Option<Phase>,
    pub outcome: Option<O>,
    pub dispatched: bool,
    pub bypasses: u8,
    pub family: usize,
    pub reserved: u64,
    pub(crate) operation: Operation,
    pub(crate) context: CallContext,
    pub(crate) plan: Option<Arc<Plan>>,
}
impl<O> Outcome<O> {
    fn map<T>(self, f: impl FnOnce(O) -> T) -> Outcome<T> {
        Outcome {
            mode: self.mode,
            request: self.request,
            response: self.response,
            outcome: self.outcome.map(f),
            dispatched: self.dispatched,
            bypasses: self.bypasses,
            family: self.family,
            reserved: self.reserved,
            operation: self.operation,
            context: self.context,
            plan: self.plan,
        }
    }
}
/// One owned, non-cloneable call captures the mode and baseline generations.
pub struct Call<'a> {
    context: CallContext,
    operation: Operation,
    audit_ref: &'a str,
    work: usize,
    inference: u64,
    delivery: Option<agw_stats::router::DeliveryContext>,
    plan: Option<Arc<Plan>>,
    gate: Option<AvailabilityGate>,
    workflow: Option<crate::cross_endpoint::Workflow>,
}
impl Call<'_> {
    /// Explicit host metadata, validated before any call executes. No transport
    /// is invoked here; invalid metadata cannot contaminate another tenant.
    pub fn with_delivery(
        mut self,
        context: agw_stats::router::DeliveryContext,
    ) -> Result<Self, agw_stats::router::Error> {
        if self.delivery.is_some()
            || self
                .plan
                .as_ref()
                .is_some_and(|plan| plan.identity.tenant != context.tenant)
        {
            return Err(agw_stats::router::Error::InvalidInput);
        }
        let checked = agw_stats::router::CallBudget::new(self.audit_ref, self.inference)?
            .with_delivery(context.clone())?;
        let _ = checked;
        self.delivery = Some(context);
        Ok(self)
    }

    /// Attach a pre-acquired local snapshot; no store access occurs during dispatch.
    pub fn with_workflow(
        mut self,
        snapshot: Result<
            agw_invariants::cross_endpoint::Snapshot,
            agw_invariants::cross_endpoint::Error,
        >,
        sequence: u64,
    ) -> Self {
        self.workflow = Some(crate::cross_endpoint::Workflow { snapshot, sequence });
        self
    }
    pub fn execute<'b, O, F>(
        self,
        request: Body<'_>,
        outbox: Option<&mut Outbox>,
        operation: F,
    ) -> Outcome<O>
    where
        F: FnOnce() -> (O, Body<'b>),
    {
        self.execute_with(request, outbox, &Builtins, operation)
    }
    /// Borrowed-capture convenience; dispatch() also supports bodies owned by
    /// the original upstream result without self-references or a clone.
    pub fn execute_with<'b, O, F, D: Detectors>(
        self,
        request: Body<'_>,
        outbox: Option<&mut Outbox>,
        detectors: &D,
        operation: F,
    ) -> Outcome<O>
    where
        F: FnOnce() -> (O, Body<'b>),
    {
        self.dispatch_with(request, outbox, detectors, operation, |pair| pair.1)
            .map(|pair| pair.0)
    }
    /// Run upstream once and inspect a borrowed capture from its owned outcome.
    /// Upstream and the host's capture projection run outside engine recovery.
    /// A transport error can project a truncated body; no outcome is replaced.
    pub fn dispatch<O, F, B>(
        self,
        request: Body<'_>,
        outbox: Option<&mut Outbox>,
        operation: F,
        capture: B,
    ) -> Outcome<O>
    where
        F: FnOnce() -> O,
        B: for<'b> FnOnce(&'b O) -> Body<'b>,
    {
        self.dispatch_with(request, outbox, &Builtins, operation, capture)
    }
    /// Injection seam for native detector/parser fault tests.
    pub fn dispatch_with<O, F, B, D: Detectors>(
        self,
        request: Body<'_>,
        mut outbox: Option<&mut Outbox>,
        detectors: &D,
        operation: F,
        capture: B,
    ) -> Outcome<O>
    where
        F: FnOnce() -> O,
        B: for<'b> FnOnce(&'b O) -> Body<'b>,
    {
        let session = self.start_with(request, outbox.as_deref_mut(), detectors);
        if session.request().decision == Decision::Halt {
            return session.complete(None, None, outbox, detectors);
        }
        let original = operation();
        let body = if session.needs_response() {
            Some(capture(&original))
        } else {
            None
        };
        session
            .complete(Some(()), body, outbox, detectors)
            .map(|()| original)
    }
}
