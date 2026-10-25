//! TI-25: explicit agent feedback, runtime contract §§7 and 10.35.
//! Hosts deliver halt payloads and attach passthrough notifications separately
//! from upstream bytes. This module performs no transport, retry or repair.
mod hint;
pub mod recovery;

use crate::gated::Outcome;
use agw_audit::contracts::audit_artifact::{
    AuditArtifactDecision as Verdict, AuditArtifactMode as Mode, DriftClass,
};
use agw_policy::Decision;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Exactly the HTTP error taxonomy in contract §7. `Internal` represents any
/// AGINDOW 5xx: clients must bypass, never promote it to a policy halt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    InvalidRequest,
    Unauthorized,
    PlanLimit,
    DriftBlocked,
    DriftHalted,
    ContractViolation,
    RateLimited,
    Internal,
}
impl ErrorCode {
    pub const fn http_status(self) -> u16 {
        match self {
            Self::InvalidRequest => 400,
            Self::Unauthorized => 401,
            Self::PlanLimit => 403,
            Self::DriftBlocked | Self::DriftHalted => 409,
            Self::ContractViolation => 422,
            Self::RateLimited => 429,
            Self::Internal => 500,
        }
    }
}
/// Passthrough is an additive notification (§4.3), not an HTTP error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeedbackCode {
    DriftBlocked,
    DriftHalted,
    DriftPassthrough,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Payload {
    pub error: FeedbackCode,
    pub tool: String,
    pub drift_class: DriftClass,
    pub drift_id: String,
    pub hint: String,
    pub audit: String,
}
impl fmt::Display for Payload {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {} ({})", self.tool, self.hint, self.audit)
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Feedback {
    /// `None` requires preserving the upstream HTTP status and body.
    pub http_status: Option<u16>,
    pub payload: Payload,
}
impl Feedback {
    /// Explicit off-path projection of an admitted TI-24 outcome. The original
    /// outcome, including completed writes and engine faults, remains borrowed.
    pub fn from_outcome<O>(outcome: &Outcome<'_, O>) -> Option<Self> {
        let audit = &outcome.audit;
        if outcome.bypasses > 0 || audit.mode == Mode::Observe {
            return None;
        }
        let error = match audit.decision {
            Verdict::Blocked if audit.mode == Mode::Strict => FeedbackCode::DriftHalted,
            Verdict::Blocked => FeedbackCode::DriftBlocked,
            Verdict::Degraded | Verdict::Bypassed if audit.drift.is_some() => {
                FeedbackCode::DriftPassthrough
            }
            _ => return None,
        };
        let phase = std::iter::once(&outcome.request)
            .chain(outcome.response.as_ref())
            .filter(|p| !p.report.changes.is_empty())
            .max_by_key(|p| p.decision)?;
        let class = phase.report.class()?;
        let dispatched = outcome.request.decision != Decision::Halt;
        let drift_id = audit.id.to_string();
        Some(Self {
            http_status: match error {
                FeedbackCode::DriftBlocked => Some(ErrorCode::DriftBlocked.http_status()),
                FeedbackCode::DriftHalted => Some(ErrorCode::DriftHalted.http_status()),
                FeedbackCode::DriftPassthrough => None,
            },
            payload: Payload {
                error,
                tool: audit.tool.to_string(),
                drift_class: match class {
                    agw_diff::DriftClass::Shape => DriftClass::Shape,
                    agw_diff::DriftClass::Semantic => DriftClass::Semantic,
                    agw_diff::DriftClass::Unknown => DriftClass::Unknown,
                },
                hint: hint::build(&phase.report, class, dispatched),
                audit: format!(
                    "https://api.agindow.com/v1/audit/{}",
                    path_segment(&drift_id)
                ),
                drift_id,
            },
        })
    }
}
fn path_segment(value: &str) -> String {
    value
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
                char::from(byte).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect()
}
