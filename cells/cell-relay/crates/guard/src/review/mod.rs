//! TI-28 off-path human review; runtime contract §10.38. Publication failures
//! never change an existing policy result. No notification or execution occurs.
mod outbox;
mod wire;
use crate::gated::{Outcome, Phase};
use agw_audit::contracts::audit_artifact::{
    AuditArtifact, AuditArtifactDecision as Verdict, AuditArtifactMode as Mode,
};
pub use agw_audit::contracts::review_case::ReviewCase;
use agw_policy::Decision;
pub use outbox::Outbox;
use std::fmt;
pub const MAX_BYTES: usize = 128 * 1024;
#[derive(Debug, Clone, Copy, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Verification {
    NotRun,
    Passed,
    Failed,
}
#[derive(Debug, Clone, serde::Serialize)]
pub struct Scope {
    pub tenant: String,
    pub connection: String,
    pub vendor: String,
    pub operation: String,
    pub contract_version: String,
    pub repair_id: String,
}
impl Scope {
    fn validate(&self, audit: &AuditArtifact) -> Result<(), Error> {
        for value in [
            &self.tenant,
            &self.connection,
            &self.vendor,
            &self.operation,
            &self.contract_version,
            &self.repair_id,
        ] {
            if !identifier(value) {
                return Err(Error::InvalidScope);
            }
        }
        if self.contract_version != audit.proof.contract_version.as_str() {
            return Err(Error::InvalidScope);
        }
        Ok(())
    }
}
pub(crate) fn identifier(value: &str) -> bool {
    !value.trim().is_empty() && value.chars().count() <= 256 && !value.chars().any(char::is_control)
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    InvalidScope,
    ResourceLimit,
    InvalidEnvelope,
    Conflict,
    Full,
    UnexpectedAck,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "human review: {self:?}")
    }
}
impl std::error::Error for Error {}
#[derive(Debug, Clone)]
pub struct Evidence {
    phases: Vec<PhaseEvidence>,
    dispatched: bool,
}
#[derive(Debug, Clone)]
struct PhaseEvidence {
    side: &'static str,
    report: agw_diff::Report,
    proposals: Vec<agw_repair::Candidate>,
    assessments: Vec<agw_repair::Assessment>,
    verification: Verification,
    decision: Decision,
    confidence: Option<f64>,
    reason: &'static str,
}
impl PhaseEvidence {
    fn from_phase(phase: &Phase<'_>, side: &'static str) -> Self {
        Self {
            side,
            report: phase.report.clone(),
            proposals: phase.proposals.clone(),
            assessments: phase.assessments.clone(),
            verification: phase.verification,
            decision: phase.decision,
            confidence: phase.confidence,
            reason: phase.reason,
        }
    }
}
/// Bounded copies of admitted control evidence only, never original body bytes.
/// Call after execution, outside inspection. Failure remains explicit in TI-27.
pub fn capture<O>(outcome: &Outcome<'_, O>) -> Result<Option<Evidence>, Error> {
    if outcome.audit.mode != Mode::Strict
        || outcome.audit.decision != Verdict::Blocked
        || outcome.bypasses != 0
    {
        return Ok(None);
    }
    let phases = std::iter::once(&outcome.request).chain(outcome.response.as_ref());
    let mut size = 0usize;
    for phase in phases {
        if phase.report.changes.len() > 4096 || phase.proposals.len() > 64 {
            return Err(Error::ResourceLimit);
        }
        for change in &phase.report.changes {
            size = size
                .saturating_add(256)
                .saturating_add(change.path.len())
                .saturating_add(change.to_path.as_ref().map_or(0, String::len));
        }
        for proposal in &phase.proposals {
            let text = match &proposal.operation {
                agw_repair::Operation::Rename { to } => to.len(),
                agw_repair::Operation::Replace(bytes) => bytes.len().saturating_mul(2),
            };
            size = size.saturating_add(1024).saturating_add(text);
        }
        if size > MAX_BYTES {
            return Err(Error::ResourceLimit);
        }
    }
    let mut phases = vec![PhaseEvidence::from_phase(&outcome.request, "request")];
    if let Some(response) = &outcome.response {
        phases.push(PhaseEvidence::from_phase(response, "response"));
    }
    Ok(Some(Evidence {
        phases,
        dispatched: outcome.request.decision != Decision::Halt,
    }))
}
pub fn case<O>(outcome: &Outcome<'_, O>, scope: &Scope) -> Result<Option<ReviewCase>, Error> {
    capture(outcome)?
        .map(|evidence| wire::build(&outcome.audit, &evidence, scope))
        .transpose()
}
pub fn case_from_completion<O>(
    outcome: &crate::idempotency::Completion<O>,
    scope: &Scope,
) -> Result<Option<ReviewCase>, Error> {
    outcome
        .review
        .as_ref()
        .map_err(|error| *error)?
        .as_ref()
        .map(|evidence| wire::build(&outcome.audit, evidence, scope))
        .transpose()
}
