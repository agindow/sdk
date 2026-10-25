//! Regenerate the cross-language fixture through the real local publisher.
use agw_audit::{
    contracts::audit_artifact::AuditArtifact,
    publish::{Publisher, PublisherConfig},
    sink::NdjsonSink,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let artifact: AuditArtifact = serde_json::from_str(include_str!(
        "../../../../../contracts/examples/audit_artifact.valid.json"
    ))?;
    let mut publisher = Publisher::new("acme", PublisherConfig::default())?;
    let now = artifact.ts;
    publisher.publish(artifact, now)?;
    publisher.drain(&mut NdjsonSink::new(std::io::stdout()), 1);
    Ok(())
}
