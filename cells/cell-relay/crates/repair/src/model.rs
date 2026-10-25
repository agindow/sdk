//! Per-proposal evidence and policy handoff values.
use std::borrow::Cow;

pub use agw_audit::contracts::audit_artifact::AuditArtifactMode as Mode;
use agw_audit::contracts::audit_artifact::{AuditArtifact, Patch};
use agw_diff::{Change, DriftClass};

/// Detector/applicability evidence comes from the caller. Inferred
/// node presence is not repair confidence. Neither input may be NaN/infinite.
#[derive(Debug, Clone, Copy)]
pub struct Evidence {
    pub detector: f64,
    pub applicability: f64,
}
impl Evidence {
    pub fn confidence(self) -> Option<f64> {
        [self.detector, self.applicability]
            .iter()
            .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
            .then(|| self.detector.min(self.applicability))
    }
}

#[derive(Debug, Clone)]
pub enum Operation {
    Rename {
        to: String,
    },
    /// One complete raw JSON scalar token. No automatic numeric arithmetic.
    Replace(Vec<u8>),
}

#[derive(Debug, Clone)]
pub struct Candidate {
    pub finding: usize,
    pub operation: Operation,
    pub evidence: Evidence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reject {
    InvalidEvidence,
    BelowThreshold,
    UnknownDrift,
    UnconfirmedIdentity,
    NumericCoercion,
    Unsupported,
    InvalidFinding,
    MissingTarget,
    Collision,
    Overlap,
    InvalidReplacement,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Eligible,
    Applied,
    Rejected(Reject),
    Observed,
    Noop,
    NotApplied,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Assessment {
    pub finding: usize,
    pub confidence: Option<f64>,
    pub class: DriftClass,
    pub status: Status,
}

/// No-application reason, separate from drift. Internal faults are bypass
/// inputs; complete malformed-body findings retain the caller's §4.4 policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    Incomplete,
    Unparseable,
    InspectionMismatch,
    DuplicateKeys,
    ResourceLimit,
    InvalidReport,
}

#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub max_bytes: usize,
    pub max_nodes: usize,
    pub max_depth: usize,
    pub max_proposals: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            max_bytes: agw_inspector::DEFAULT_CAP,
            max_nodes: 4096,
            max_depth: 64,
            max_proposals: 64,
        }
    }
}

/// Unchanged outcomes borrow the exact caller slice. Applied outcomes own only
/// the patched body. Unresolved findings still require a policy decision.
#[derive(Debug)]
pub struct Outcome<'a> {
    pub body: Cow<'a, [u8]>,
    pub assessments: Vec<Assessment>,
    pub unresolved: Vec<Change>,
    pub failure: Option<Failure>,
    pub(crate) patch: Option<Patch>,
    pub(crate) mode: Mode,
}
impl Outcome<'_> {
    pub fn patch(&self) -> Option<&Patch> {
        self.patch.as_ref()
    }
    pub fn confidence(&self) -> Option<f64> {
        self.assessments
            .iter()
            .filter(|a| a.status == Status::Applied)
            .filter_map(|a| a.confidence)
            .reduce(f64::min)
    }
    pub fn class(&self) -> Option<DriftClass> {
        self.unresolved
            .iter()
            .map(|c| c.class)
            .max_by_key(|class| match class {
                DriftClass::Shape => 0,
                DriftClass::Unknown => 1,
                DriftClass::Semantic => 2,
            })
    }
    /// Stamp repair evidence only. Final decision and write-integrity proof
    /// remain with the caller's policy/verifier; this is not publication.
    pub fn audit(&self, mut draft: AuditArtifact) -> AuditArtifact {
        draft.patch = self.patch.clone();
        draft.confidence = self.confidence();
        draft.mode = self.mode;
        draft
    }
}
