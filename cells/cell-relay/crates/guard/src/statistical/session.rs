//! A call-owned suspension point for asynchronous hosts; no upstream value enters it.
use super::{Body, Builtins, Call, Decision, Detectors, Outcome, Phase};
use crate::{
    availability::{Budget, EngineFault},
    readiness::{Evaluation, MonotonicClock},
};
use agw_diff::Side;
use agw_stats::router::{CallBudget, Outbox};
use std::panic::AssertUnwindSafe;

pub struct Session<'a> {
    call: Call<'a>,
    work: Budget,
    inference: Option<CallBudget>,
    permit: Option<Evaluation<MonotonicClock>>,
    denied: Option<EngineFault>,
    request: Phase,
}
impl<'a> Call<'a> {
    pub fn start(self, request: Body<'_>) -> Session<'a> {
        self.start_with(request, None, &Builtins)
    }
    pub(super) fn start_with(
        self,
        request: Body<'_>,
        outbox: Option<&mut Outbox>,
        detectors: &impl Detectors,
    ) -> Session<'a> {
        let (permit, denied) = match self.gate.as_ref().map(|g| g.admit()) {
            Some(Ok(permit)) => (Some(permit), None),
            Some(Err(fault)) => (None, Some(fault)),
            None => (None, None),
        };
        let mut session = Session {
            work: Budget::new(self.work),
            call: self,
            inference: None,
            permit,
            denied,
            request: Phase::clean(),
        };
        session.request = session.phase(request, Side::Request, outbox, detectors);
        if session.request.decision == Decision::Halt {
            session.request.cross_observations.clear();
        }
        session
    }
}
impl Session<'_> {
    pub fn request(&self) -> &Phase {
        &self.request
    }
    pub(super) fn needs_response(&self) -> bool {
        self.request.fault.is_none() && self.call.plan.is_some()
    }
    pub fn finish(self, response: Body<'_>) -> Outcome<()> {
        let original = (self.request.decision != Decision::Halt).then_some(());
        self.complete(original, Some(response), None, &Builtins)
    }
    /// Finish a completed operation whose host capture could not be admitted.
    /// Mark one availability failure through the existing permit so audit and
    /// breaker accounting agree with the host's bypass; never alter a halt.
    pub fn finish_unavailable(mut self, response: Body<'_>) -> Outcome<()> {
        if self.call.plan.is_some()
            && self.request.decision != Decision::Halt
            && self.request.fault.is_none()
            && let Some(permit) = self.permit.as_mut()
        {
            let _ = permit.evaluate(AssertUnwindSafe(|| {
                Err::<(), _>(EngineFault::ResourceExhausted)
            }));
            self.denied = Some(EngineFault::ResourceExhausted);
        }
        self.finish(response)
    }
    fn phase(
        &mut self,
        body: Body<'_>,
        side: Side,
        outbox: Option<&mut Outbox>,
        detectors: &impl Detectors,
    ) -> Phase {
        if let Some(fault) = self.denied {
            return Phase::bypass(fault);
        }
        let (Some(plan), Some(permit)) = (&self.call.plan, &mut self.permit) else {
            return Phase::clean();
        };
        permit
            .evaluate(AssertUnwindSafe(|| {
                if self.inference.is_none() {
                    self.work
                        .charge(self.call.audit_ref.len().saturating_add(4096))?;
                    if self.call.audit_ref.is_empty() || self.call.audit_ref.len() > 1024 {
                        return Err(EngineFault::Internal);
                    }
                    let qualified = format!(
                        "{}:{}:{}",
                        plan.identity.tenant.len(),
                        plan.identity.tenant,
                        self.call.audit_ref
                    );
                    self.inference = Some(match &self.call.delivery {
                        Some(context) => CallBudget::new(self.call.audit_ref, self.call.inference)
                            .and_then(|budget| budget.with_delivery(context.clone()))
                            .map_err(|_| EngineFault::Internal)?,
                        None => CallBudget::new(&qualified, self.call.inference)
                            .map_err(|_| EngineFault::Internal)?,
                    });
                }
                super::evaluate::phase(
                    plan,
                    self.call.audit_ref,
                    self.call.workflow.as_ref(),
                    self.call.context,
                    self.call.operation,
                    body,
                    side,
                    detectors,
                    &mut self.work,
                    outbox,
                    self.inference.as_mut().ok_or(EngineFault::Internal)?,
                )
            }))
            .unwrap_or_else(Phase::bypass)
    }
    pub(super) fn complete<O>(
        mut self,
        original: Option<O>,
        response: Option<Body<'_>>,
        outbox: Option<&mut Outbox>,
        detectors: &impl Detectors,
    ) -> Outcome<O> {
        let response = if original.is_none() || self.request.fault.is_some() {
            None
        } else if self.call.plan.is_none() {
            Some(Phase::clean())
        } else {
            response.map(|body| self.phase(body, Side::Response, outbox, detectors))
        };
        let bypasses = u8::from(
            self.request.fault.is_some() || response.as_ref().is_some_and(|p| p.fault.is_some()),
        );
        Outcome {
            mode: self.call.context.mode(),
            request: self.request,
            response,
            dispatched: original.is_some(),
            outcome: original,
            bypasses,
            family: self.call.plan.as_ref().map_or(0, |p| p.family()),
            reserved: self.inference.as_ref().map_or(0, CallBudget::reserved),
            operation: self.call.operation,
            context: self.call.context,
            plan: self.call.plan,
        }
    }
}
