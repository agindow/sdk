//! Shared fixtures for the TI-29 audit publication tests.
#![allow(dead_code)]

use agw_audit::contracts::audit_artifact::AuditArtifact;
use std::error::Error as StdError;

type Result = std::result::Result<AuditArtifact, Box<dyn StdError>>;

/// Clock resolution cannot distinguish concurrent test fixtures (OPS-16).
pub fn spool_path() -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    std::env::temp_dir().join(format!(
        "ti57-{}-{}.json",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}

/// The same §3.3 fixture every other suite replays.
pub fn fixture() -> Result {
    Ok(serde_json::from_str(include_str!(
        "../../../../../../contracts/examples/audit_artifact.valid.json"
    ))?)
}

/// A clean read, the shape a no-drift call publishes.
pub fn clean_fixture() -> Result {
    Ok(serde_json::from_str(include_str!(
        "../../../../../../contracts/examples/audit_artifact.clean.valid.json"
    ))?)
}

/// An availability bypass: no drift, no patch, original outcome retained.
pub fn bypassed() -> Result {
    let mut artifact = fixture()?;
    artifact.id = "aud_bypassed".try_into()?;
    artifact.decision = agw_audit::contracts::audit_artifact::AuditArtifactDecision::Bypassed;
    artifact.drift = None;
    artifact.patch = None;
    artifact.detail = Some("availability_bypass".try_into()?);
    Ok(artifact)
}

/// A blocked write: drift recorded, nothing applied.
pub fn blocked() -> Result {
    let mut artifact = fixture()?;
    artifact.id = "aud_blocked".try_into()?;
    artifact.decision = agw_audit::contracts::audit_artifact::AuditArtifactDecision::Blocked;
    artifact.patch = None;
    Ok(artifact)
}

/// A sink that retains everything for assertions.
pub struct MemorySink(pub Vec<AuditArtifact>);

impl agw_audit::sink::Sink for MemorySink {
    fn emit(
        &mut self,
        artifact: &AuditArtifact,
    ) -> std::result::Result<(), agw_audit::sink::SinkError> {
        self.0.push(artifact.clone());
        Ok(())
    }
}

/// A sink whose delivery always fails, standing in for an unreachable log sink.
pub struct FailingSink;

impl agw_audit::sink::Sink for FailingSink {
    fn emit(
        &mut self,
        _artifact: &AuditArtifact,
    ) -> std::result::Result<(), agw_audit::sink::SinkError> {
        Err(agw_audit::sink::SinkError::new("offline"))
    }
}
