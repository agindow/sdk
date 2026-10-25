//! TI-24: explicit bounded-capture execution, runtime contract §10.34.
//! Local callbacks supply evidence and independent payload verification. Hosts
//! deliver returned effective bytes and honor response halts; upstream outcomes
//! remain available even when a completed response is halted. No retries or I/O.
mod artifact;
mod phase;

use crate::{
    availability::{Budget, EngineFault},
    config::CallContext,
    proxy::Headers,
    readiness::AvailabilityGate,
};
use agw_audit::contracts::audit_artifact::{AuditArtifact, Patch};
use agw_diff::{Report, Side};
use agw_policy::Decision;
use std::{borrow::Cow, panic::AssertUnwindSafe};

#[derive(Clone, Copy)]
pub struct Body<'a> {
    pub bytes: &'a [u8],
    pub truncated: bool,
}
impl<'a> Body<'a> {
    pub fn complete(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            truncated: false,
        }
    }
}
#[derive(Default)]
pub struct Evidence {
    pub report: Report,
    pub candidates: Vec<agw_repair::Candidate>,
}
/// Callbacks must be local, bounded and charge before work/allocation. `verify`
/// checks each supplied applied finding against independent authoritative
/// evidence, not just syntactic JSON validity. False is a normal policy input;
/// errors/panics are availability faults. No automatic verifier is installed.
pub trait Pipeline {
    fn detect(&self, side: Side, bytes: &[u8], work: &mut Budget) -> Result<Evidence, EngineFault>;
    fn verify(
        &self,
        side: Side,
        patched: &[u8],
        findings: &[usize],
        work: &mut Budget,
    ) -> Result<bool, EngineFault>;
}
#[derive(Debug)]
pub struct Phase<'a> {
    pub body: Cow<'a, [u8]>,
    pub report: Report,
    pub decision: Decision,
    pub fault: Option<EngineFault>,
    pub reason: &'static str,
    pub patch: Option<Patch>,
    pub confidence: Option<f64>,
    pub(crate) proposals: Vec<agw_repair::Candidate>,
    pub(crate) assessments: Vec<agw_repair::Assessment>,
    pub(crate) verification: crate::review::Verification,
    pub(crate) malformed: bool,
    pub(crate) changes: Vec<agw_audit::contracts::audit_artifact::Change>,
}
impl<'a> Phase<'a> {
    fn bypass(body: Body<'a>, fault: EngineFault) -> Self {
        Self {
            body: Cow::Borrowed(body.bytes),
            report: Report::default(),
            decision: Decision::Pass,
            fault: Some(fault),
            reason: "availability_bypass",
            patch: None,
            confidence: None,
            proposals: Vec::new(),
            assessments: Vec::new(),
            verification: crate::review::Verification::NotRun,
            malformed: false,
            changes: Vec::new(),
        }
    }
}
/// A response halt cannot undo a dispatched write. The original upstream value
/// is retained independently from the effective response capture.
#[must_use = "Honor the final policy disposition and deliver effective bytes"]
#[derive(Debug)]
pub struct Outcome<'a, O> {
    pub request: Phase<'a>,
    pub response: Option<Phase<'a>>,
    pub outcome: Option<O>,
    pub bypasses: u8,
    pub audit: AuditArtifact,
    pub headers: Headers,
    pub warning: Option<&'static str>,
}
pub struct Call<'g> {
    context: CallContext,
    gate: &'g AvailabilityGate,
    work: usize,
}
impl<'g> Call<'g> {
    pub fn new(context: CallContext, gate: &'g AvailabilityGate, work: usize) -> Self {
        Self {
            context,
            gate,
            work,
        }
    }
    /// Caller execution is outside recovery. Request faults pass original bytes
    /// once and skip response evaluation. Response faults preserve sent patches.
    pub fn execute<'a, O>(
        self,
        request: Body<'a>,
        pipeline: &impl Pipeline,
        draft: AuditArtifact,
        operation: impl FnOnce(&[u8]) -> (O, Body<'a>),
    ) -> Outcome<'a, O> {
        let mut work = Budget::new(self.work);
        let admission = self.gate.admit();
        let (mut permit, denied) = match admission {
            Ok(permit) => (Some(permit), None),
            Err(fault) => (None, Some(fault)),
        };
        let mut evaluate = |body, side| {
            if let Some(fault) = denied {
                return Phase::bypass(body, fault);
            }
            let Some(permit) = &mut permit else {
                return Phase::bypass(body, EngineFault::Internal);
            };
            permit
                .evaluate(AssertUnwindSafe(|| {
                    let phase = phase::evaluate(
                        self.context,
                        draft.operation,
                        body,
                        side,
                        pipeline,
                        &mut work,
                    )?;
                    // Ignoring a callback's failed budget charge must not restore policy.
                    work.charge(0)?;
                    Ok(phase)
                }))
                .unwrap_or_else(|fault| Phase::bypass(body, fault))
        };
        let request = evaluate(request, Side::Request);
        let (outcome, response) = if request.decision == Decision::Halt {
            (None, None)
        } else {
            let (original, body) = operation(request.body.as_ref());
            let response = if request.fault.is_some() {
                None
            } else {
                Some(evaluate(body, Side::Response))
            };
            (Some(original), response)
        };
        let bypasses = u8::from(
            request.fault.is_some() || response.as_ref().is_some_and(|p| p.fault.is_some()),
        );
        let (audit, headers, warning) =
            artifact::compose(self.context, draft, &request, response.as_ref(), bypasses);
        Outcome {
            request,
            response,
            outcome,
            bypasses,
            audit,
            headers,
            warning,
        }
    }
}
