//! Audit artifact construction — what was sent, what changed, and why.
//!
//! Inherited constraints, not renegotiable in this crate:
//! - artifacts are immutable and append-only (`TECH_STACK.md` §7);
//! - credentials never enter the artifact — connection handles only;
//! - propagate with `?`; the workspace lints deny `unwrap`/`expect` in library
//!   code (`AGENTS.md` §3.1).
//!
//! TI-29 adds the local publication seam: honesty validation, recurrence
//! stamping, an append-only bounded store, a host-driven sink outbox and
//! OTLP/JSON metrics. Delivery I/O is an explicit native host operation; no guard call
//! awaits publication.

pub mod artifact;
#[cfg(not(target_arch = "wasm32"))]
pub mod delivery;
pub mod investigation_wire;
pub mod otel;
pub mod profile_wire;
pub mod publish;
pub mod recurrence;
pub mod sink;
pub mod source_repair_wire;
pub mod store;

/// Invalid artifacts and exhausted bounds. Publication is best-effort: callers
/// may ignore this value, and no variant turns into a thrown guard error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// A supplied value violates a documented rule; the reason is stable.
    Invalid(&'static str),
    /// An artifact ID already exists with different bytes.
    Conflict,
    /// A bounded tracker or store is full; nothing was evicted.
    Capacity,
    /// The store's cumulative byte ceiling would be exceeded.
    ByteLimit,
    /// The artifact is older than the configured retention.
    Expired,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(reason) => write!(f, "invalid audit artifact: {reason}"),
            other => write!(f, "{other:?}"),
        }
    }
}

impl std::error::Error for Error {}

/// Shared tenant/identifier rule: 1–256 bytes, no control characters.
pub(crate) fn valid_identifier(value: &str) -> bool {
    !value.is_empty() && value.len() <= 256 && !value.chars().any(char::is_control)
}

/// Clone a generated `String` newtype (typify newtypes have no `Display`).
pub(crate) fn text<T: std::ops::Deref<Target = String>>(value: &T) -> String {
    String::clone(value)
}

/// Cross-cell message types, generated from `contracts/*.v1.json` at build
/// time by `build.rs` (typify) — TI-02 AC 5. Hand-writing these is impossible
/// by construction: the schemas are the source of truth and cargo re-runs the
/// generator whenever one changes.
pub mod contracts {
    include!(concat!(env!("OUT_DIR"), "/contracts.rs"));
}

#[cfg(test)]
mod tests {
    use super::contracts::audit_artifact::AuditArtifact;
    use super::contracts::contract_snapshot::ContractSnapshot;
    use super::contracts::drift_event::DriftEvent;
    use super::contracts::finding::Finding;

    // The same fixtures the TypeScript suites replay, so both languages are
    // held to one definition of "a valid message".
    const DRIFT_EVENT: &str =
        include_str!("../../../../../contracts/examples/drift_event.valid.json");
    const DRIFT_EVENT_INVALID: &str =
        include_str!("../../../../../contracts/examples/drift_event.invalid.json");
    const CONTRACT_SNAPSHOT: &str =
        include_str!("../../../../../contracts/examples/contract_snapshot.valid.json");
    const CONTRACT_SNAPSHOT_INVALID: &str =
        include_str!("../../../../../contracts/examples/contract_snapshot.invalid.json");
    const FINDING: &str = include_str!("../../../../../contracts/examples/finding.valid.json");
    const FINDING_INVALID: &str =
        include_str!("../../../../../contracts/examples/finding.invalid.json");
    const AUDIT_ARTIFACT: &str =
        include_str!("../../../../../contracts/examples/audit_artifact.valid.json");
    const AUDIT_ARTIFACT_INVALID: &str =
        include_str!("../../../../../contracts/examples/audit_artifact.invalid.json");

    #[test]
    fn valid_messages_deserialize() -> Result<(), serde_json::Error> {
        let _: DriftEvent = serde_json::from_str(DRIFT_EVENT)?;
        let _: ContractSnapshot = serde_json::from_str(CONTRACT_SNAPSHOT)?;
        let _: Finding = serde_json::from_str(FINDING)?;
        let _: AuditArtifact = serde_json::from_str(AUDIT_ARTIFACT)?;
        Ok(())
    }

    #[test]
    fn invalid_messages_are_rejected() {
        assert!(serde_json::from_str::<DriftEvent>(DRIFT_EVENT_INVALID).is_err());
        assert!(serde_json::from_str::<ContractSnapshot>(CONTRACT_SNAPSHOT_INVALID).is_err());
        assert!(serde_json::from_str::<Finding>(FINDING_INVALID).is_err());
        assert!(serde_json::from_str::<AuditArtifact>(AUDIT_ARTIFACT_INVALID).is_err());
    }
}

pub mod dashboard_wire;
pub mod finding_wire;

pub mod credential_wire;
