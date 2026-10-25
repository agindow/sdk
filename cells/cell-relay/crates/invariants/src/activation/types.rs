use crate::{Form, Rule, store};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Invalid,
    Capacity,
    Conflict,
    Compilation,
    Expired,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scope {
    pub tenant: String,
    pub connection: String,
    pub vendor: String,
    pub operation: String,
    pub contract: u64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Shadow,
    Reads,
    Blocking,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RolloutMode {
    Observe,
    Repair,
    Strict,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    Range,
    Enumeration,
    Relation,
    Ordering,
    Sum,
    CrossEndpoint,
}
impl Family {
    pub(super) fn of(rule: &Rule) -> Self {
        match rule.form {
            Form::Range { .. } => Self::Range,
            Form::Enum { .. } => Self::Enumeration,
            Form::Relation { .. } => Self::Relation,
            Form::Ordering { .. } => Self::Ordering,
            Form::Sum { .. } => Self::Sum,
            Form::CrossEndpoint { .. } => Self::CrossEndpoint,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub scope: Scope,
    pub version: u64,
    pub rules: Vec<Rule>,
    pub bindings: Vec<String>,
    pub peers: Vec<crate::cross_endpoint::Rule>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attestation {
    pub rule: Rule,
    /// Explicit business bounds or relationship/concept/unit/revision semantics.
    pub semantics: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerAttestation {
    pub rule: crate::cross_endpoint::Rule,
    pub revision_semantics: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Thresholds {
    pub minimum_valid: u64,
    pub minimum_invalid: u64,
    pub coverage_ppm: u32,
    pub false_positive_ppm: u32,
    pub missed_violation_ppm: u32,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counts {
    pub eligible: u64,
    pub evaluated: u64,
    pub valid: u64,
    pub invalid: u64,
    pub false_positives: u64,
    pub missed_violations: u64,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Calibration {
    pub dataset: String,
    pub candidate: u64,
    pub sampling: String,
    pub independent_production: bool,
    pub counts: Counts,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Policy {
    pub scope: Scope,
    pub revision: u64,
    pub expires: u64,
    pub families: Vec<Family>,
    pub stages: Vec<Stage>,
    pub modes: Vec<RolloutMode>,
    pub versions: (u64, u64),
    pub thresholds: Thresholds,
    pub attestations: Vec<Attestation>,
    pub peer_attestations: Vec<PeerAttestation>,
    pub calibration: Option<Calibration>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Receipt {
    pub id: u64,
    pub scope: Scope,
    pub candidate: u64,
    pub policy: u64,
    pub stage: Stage,
    pub training: String,
    pub holdout: String,
    pub independent: bool,
    pub complete: bool,
    pub started: u64,
    pub finished: u64,
    pub expires: u64,
    pub counts: Counts,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    Qualified,
    Unauthorized,
    Expired,
    Family,
    Attestation,
    StageOrder,
    CustomerControl,
    Insufficient,
    Incomplete,
    Contradictory,
    Negative,
    Calibration,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transition {
    Prepared,
    Published,
    Conflict,
    PublicationFailed,
    CustomerEdit,
}
#[derive(Debug, Clone)]
pub struct Decision {
    pub scope: Scope,
    pub candidate: u64,
    pub policy: Option<u64>,
    pub stage: Stage,
    pub authorization: Option<Policy>,
    pub at: u64,
    pub receipts: Vec<Receipt>,
    pub counts: Counts,
    pub reason: Reason,
    pub controls: store::Controls,
    pub transition: Transition,
}
#[derive(Debug)]
pub struct Generation {
    pub(super) origin: std::sync::Arc<()>,
    pub(super) number: u64,
    pub(super) candidate: Candidate,
    pub(super) snapshot: store::Snapshot,
    pub(super) decision: Decision,
    pub(super) peers: Vec<crate::cross_endpoint::Rule>,
}
impl Generation {
    pub fn same_controller(&self, other: &Self) -> bool {
        std::sync::Arc::ptr_eq(&self.origin, &other.origin)
    }
    pub fn peers(&self) -> &[crate::cross_endpoint::Rule] {
        &self.peers
    }
    /// Includes disabled peers so a refresh can remove previously selected rules.
    pub fn has_peers(&self) -> bool {
        !self.candidate.peers.is_empty()
    }
    pub fn number(&self) -> u64 {
        self.number
    }
    pub fn snapshot(&self) -> &store::Snapshot {
        &self.snapshot
    }
    pub fn stage(&self) -> Stage {
        self.decision.stage
    }
    pub fn decision(&self) -> &Decision {
        &self.decision
    }
}
