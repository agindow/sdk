//! Contract §3.1/§3.3 honesty rules the closed schema cannot express.
mod support;

use agw_audit::contracts::audit_artifact::Patch;
use agw_audit::{Error, artifact};
use std::error::Error as StdError;

type Result = std::result::Result<(), Box<dyn StdError>>;

#[test]
fn the_shared_fixture_satisfies_the_honesty_rules() -> Result {
    let artifact = support::fixture()?;
    artifact::validate(&artifact)?;
    assert!(artifact::byte_len(&artifact)? > 100);
    let clean = support::clean_fixture()?;
    artifact::validate(&clean)?;
    Ok(())
}

#[test]
fn a_patch_claiming_repair_and_byte_identity_is_rejected() -> Result {
    let mut artifact = support::fixture()?;
    let patch = artifact.patch.as_mut().ok_or("missing patch")?;
    patch.applied = true;
    patch.byte_identical = true;
    assert!(matches!(
        artifact::validate(&artifact),
        Err(Error::Invalid(_))
    ));
    Ok(())
}

#[test]
fn an_unapplied_patch_claiming_changed_bytes_is_rejected() -> Result {
    let mut artifact = support::fixture()?;
    let patch = artifact.patch.as_mut().ok_or("missing patch")?;
    patch.applied = false;
    patch.byte_identical = false;
    assert!(matches!(
        artifact::validate(&artifact),
        Err(Error::Invalid(_))
    ));
    Ok(())
}

#[test]
fn an_applied_patch_without_fields_is_rejected() -> Result {
    let mut artifact = support::fixture()?;
    let patch = artifact.patch.as_mut().ok_or("missing patch")?;
    patch.fields_changed.clear();
    assert!(matches!(
        artifact::validate(&artifact),
        Err(Error::Invalid(_))
    ));
    Ok(())
}

#[test]
fn a_repaired_decision_requires_an_applied_patch() -> Result {
    let mut artifact = support::fixture()?;
    artifact.patch = None;
    assert!(matches!(
        artifact::validate(&artifact),
        Err(Error::Invalid(_))
    ));
    artifact.patch = Some(Patch {
        applied: false,
        byte_identical: true,
        fields_changed: Vec::new(),
        selective: None,
        reasoning: None,
    });
    assert!(matches!(
        artifact::validate(&artifact),
        Err(Error::Invalid(_))
    ));
    Ok(())
}

#[test]
fn recurrence_fields_are_present_or_absent_together() -> Result {
    let mut artifact = support::fixture()?;
    artifact.drift.as_mut().ok_or("missing drift")?.first_seen = None;
    assert!(matches!(
        artifact::validate(&artifact),
        Err(Error::Invalid(_))
    ));

    let mut artifact = support::fixture()?;
    artifact.drift.as_mut().ok_or("missing drift")?.occurrences = None;
    assert!(matches!(
        artifact::validate(&artifact),
        Err(Error::Invalid(_))
    ));
    Ok(())
}

#[test]
fn tenant_metadata_must_be_a_bounded_identifier_when_present() -> Result {
    let mut artifact = support::fixture()?;
    artifact
        .metadata
        .insert("tenant".into(), serde_json::Value::from(""));
    assert!(matches!(
        artifact::validate(&artifact),
        Err(Error::Invalid(_))
    ));

    let mut artifact = support::fixture()?;
    artifact
        .metadata
        .insert("tenant".into(), serde_json::Value::from("x".repeat(257)));
    assert!(matches!(
        artifact::validate(&artifact),
        Err(Error::Invalid(_))
    ));

    let mut artifact = support::fixture()?;
    artifact
        .metadata
        .insert("tenant".into(), serde_json::Value::from(7));
    assert!(matches!(
        artifact::validate(&artifact),
        Err(Error::Invalid(_))
    ));
    Ok(())
}

#[test]
fn canonical_bytes_round_trip_to_the_same_artifact() -> Result {
    let artifact = support::fixture()?;
    let bytes = artifact::canonical(&artifact)?;
    let decoded: agw_audit::contracts::audit_artifact::AuditArtifact =
        serde_json::from_slice(&bytes)?;
    assert_eq!(artifact::canonical(&decoded)?, bytes);
    Ok(())
}
