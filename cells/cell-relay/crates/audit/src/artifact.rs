//! Honesty rules the closed §3.3 schema cannot express (TI-29).
//!
//! The schema carries the shape; these rules carry the meaning. An artifact
//! that claims a repair and byte identity, or a repaired decision without an
//! applied patch, is rejected before it can be stored or published — the audit
//! artifact is the product, so a false claim in it is a defect, not metadata.

use crate::Error;
use crate::contracts::audit_artifact::{AuditArtifact, AuditArtifactDecision};

/// Validate the artifact's internal honesty.
///
/// Rejects:
/// - `patch.applied` and `patch.byte_identical` that are not complementary
///   (contract §3.1: a repair always breaks byte identity);
/// - an applied patch that names no changed field;
/// - a `repaired` decision without an applied patch;
/// - `occurrences` or `first_seen` present without the other;
/// - tenant metadata that is not a bounded identifier.
pub fn validate(artifact: &AuditArtifact) -> Result<(), Error> {
    if let Some(patch) = &artifact.patch {
        if patch.applied == patch.byte_identical {
            return Err(Error::Invalid(
                "patch must be applied with changed bytes or unapplied with original bytes",
            ));
        }
        if patch.applied && patch.fields_changed.is_empty() {
            return Err(Error::Invalid("applied patch must name changed fields"));
        }
    }
    if artifact.decision == AuditArtifactDecision::Repaired
        && artifact.patch.as_ref().is_none_or(|patch| !patch.applied)
    {
        return Err(Error::Invalid(
            "repaired decision requires an applied patch",
        ));
    }
    if let Some(drift) = &artifact.drift
        && drift.occurrences.is_some() != drift.first_seen.is_some()
    {
        return Err(Error::Invalid(
            "occurrences and first_seen are present or absent together",
        ));
    }
    if let Some(tenant) = artifact.metadata.get("tenant") {
        match tenant.as_str() {
            Some(value) if crate::valid_identifier(value) => {}
            _ => {
                return Err(Error::Invalid(
                    "tenant metadata must be a bounded identifier",
                ));
            }
        }
    }
    Ok(())
}

/// The exact serialized bytes published, stored and compared for identity.
pub fn canonical(artifact: &AuditArtifact) -> Result<Vec<u8>, Error> {
    serde_json::to_vec(artifact).map_err(|_| Error::Invalid("artifact does not serialize"))
}

/// The published size in bytes.
pub fn byte_len(artifact: &AuditArtifact) -> Result<usize, Error> {
    canonical(artifact).map(|bytes| bytes.len())
}
