//! Review regressions for TI-29 admission and TI-57 direct tenant delivery.
mod support;
use agw_audit::{
    Error,
    contracts::audit_artifact::AuditArtifact,
    publish::{Publisher, PublisherConfig},
    sink::{Sink, SinkError},
    store::Limits,
};
use chrono::Duration;
type Result = std::result::Result<(), Box<dyn std::error::Error>>;

fn rejected_then_valid(rejected: AuditArtifact, expected: Error) -> Result {
    let valid = support::fixture()?;
    let now = valid.ts;
    let mut publisher = Publisher::new(
        "acme",
        PublisherConfig {
            limits: Limits {
                byte_limit: 16_384,
                ..Limits::default()
            },
            ..PublisherConfig::default()
        },
    )?;
    assert_eq!(publisher.publish(rejected, now), Err(expected));
    assert!(publisher.store().is_empty());
    assert_eq!(publisher.pending(), 0);
    let id = String::clone(&valid.id);
    publisher.publish(valid, now)?;
    let drift = publisher
        .store()
        .get(&id, now)
        .and_then(|artifact| artifact.drift.as_ref())
        .ok_or("missing drift")?;
    assert_eq!(drift.occurrences.map(u64::from), Some(1));
    assert_eq!(drift.first_seen, Some(now));
    Ok(())
}

#[test]
fn expired_publication_does_not_advance_recurrence() -> Result {
    let mut expired = support::fixture()?;
    expired.ts -= Duration::days(91);
    rejected_then_valid(expired, Error::Expired)
}

#[test]
fn dishonest_publication_does_not_advance_recurrence() -> Result {
    let mut dishonest = support::fixture()?;
    dishonest
        .patch
        .as_mut()
        .ok_or("fixture patch")?
        .byte_identical = true;
    rejected_then_valid(
        dishonest,
        Error::Invalid("patch must be applied with changed bytes or unapplied with original bytes"),
    )
}

#[test]
fn wrong_tenant_publication_does_not_advance_recurrence() -> Result {
    let mut wrong = support::fixture()?;
    wrong
        .metadata
        .insert("tenant".into(), serde_json::json!("other-tenant"));
    rejected_then_valid(
        wrong,
        Error::Invalid("artifact tenant metadata does not match the store tenant"),
    )
}

#[test]
fn oversized_publication_does_not_advance_recurrence() -> Result {
    let mut oversized = support::fixture()?;
    oversized
        .metadata
        .insert("extra".into(), serde_json::json!("x".repeat(32_768)));
    rejected_then_valid(oversized, Error::ByteLimit)
}

#[test]
fn rejected_signature_does_not_consume_recurrence_capacity() -> Result {
    let valid = support::fixture()?;
    let now = valid.ts;
    let mut rejected = valid.clone();
    rejected.ts -= Duration::days(91);
    rejected
        .drift
        .as_mut()
        .and_then(|d| d.changes.first_mut())
        .ok_or("fixture change")?
        .path = "/rejected".try_into()?;
    let mut publisher = Publisher::new(
        "acme",
        PublisherConfig {
            recurrence_capacity: 1,
            ..PublisherConfig::default()
        },
    )?;
    assert_eq!(publisher.publish(rejected, now), Err(Error::Expired));
    let id = String::clone(&valid.id);
    publisher.publish(valid, now)?;
    assert_eq!(publisher.recurrence_dropped(), 0);
    assert_eq!(
        publisher
            .store()
            .get(&id, now)
            .and_then(|a| a.drift.as_ref())
            .and_then(|d| d.occurrences)
            .map(u64::from),
        Some(1)
    );
    Ok(())
}

#[test]
fn store_refusal_does_not_count_a_recurrence_drop() -> Result {
    let valid = support::fixture()?;
    let now = valid.ts;
    let mut rejected = valid.clone();
    rejected.id = "rejected".try_into()?;
    rejected
        .drift
        .as_mut()
        .and_then(|d| d.changes.first_mut())
        .ok_or("fixture change")?
        .path = "/rejected".try_into()?;
    let mut publisher = Publisher::new(
        "acme",
        PublisherConfig {
            limits: Limits {
                capacity: 1,
                ..Limits::default()
            },
            recurrence_capacity: 1,
            ..PublisherConfig::default()
        },
    )?;
    publisher.publish(valid, now)?;
    assert_eq!(publisher.publish(rejected, now), Err(Error::Capacity));
    assert_eq!(publisher.recurrence_dropped(), 0);
    Ok(())
}

struct TenantSink<'a> {
    tenant: &'a str,
    artifacts: Vec<AuditArtifact>,
}
impl Sink for TenantSink<'_> {
    fn tenant(&self) -> Option<&str> {
        Some(self.tenant)
    }
    fn emit(&mut self, artifact: &AuditArtifact) -> std::result::Result<(), SinkError> {
        self.artifacts.push(artifact.clone());
        Ok(())
    }
}

#[test]
fn wrong_tenant_drain_retains_unattributed_artifacts_without_io() -> Result {
    let first = support::clean_fixture()?;
    assert!(!first.metadata.contains_key("tenant"));
    let now = first.ts;
    let mut second = first.clone();
    second.id = "second".try_into()?;
    let expected = serde_json::to_value([&first, &second])?;
    let mut publisher = Publisher::new("acme", PublisherConfig::default())?;
    publisher.publish(first, now)?;
    publisher.publish(second, now)?;
    let mut sink = TenantSink {
        tenant: "other",
        artifacts: vec![],
    };
    assert_eq!(publisher.drain(&mut sink, 2), 0);
    assert!(sink.artifacts.is_empty(), "tenant rejection precedes emit");
    assert_eq!(publisher.pending(), 2);
    assert_eq!(publisher.delivered(), 0);
    sink.tenant = "acme";
    assert_eq!(publisher.drain(&mut sink, 2), 2);
    assert_eq!(serde_json::to_value(&sink.artifacts)?, expected);
    assert_eq!(publisher.pending(), 0);
    assert_eq!(publisher.delivered(), 2);
    Ok(())
}
