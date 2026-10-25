//! TI-29 publication: recurrence stamping, bounded outbox, host-driven sinks.
mod support;

use agw_audit::Error;
use agw_audit::contracts::audit_artifact::AuditArtifactDecision;
use agw_audit::publish::{Publisher, PublisherConfig};
use agw_audit::sink::{NdjsonSink, Sink};
use agw_audit::store::{Append, Limits};
use chrono::{Duration, TimeZone, Utc};
use std::error::Error as StdError;

type Result = std::result::Result<(), Box<dyn StdError>>;

fn now() -> chrono::DateTime<Utc> {
    const SECONDS: i64 = 1_761_115_211;
    Utc.timestamp_opt(SECONDS, 0)
        .single()
        .unwrap_or_else(Utc::now)
}

fn publisher(
    capacity: usize,
    recurrence: usize,
    outbox: usize,
) -> std::result::Result<Publisher, Box<dyn StdError>> {
    Ok(Publisher::new(
        "acme",
        PublisherConfig {
            limits: Limits {
                capacity,
                retention: Duration::hours(1),
                ..Limits::default()
            },
            recurrence_capacity: recurrence,
            outbox_capacity: outbox,
        },
    )?)
}

#[test]
fn publication_stamps_recurrence_stores_and_queues_once() -> Result {
    let artifact = support::fixture()?;
    let mut publisher = publisher(8, 8, 8)?;
    let receipt = publisher.publish(artifact.clone(), now())?;
    assert_eq!(receipt.outcome, Append::Stored);
    assert!(receipt.queued);
    assert_eq!(publisher.pending(), 1);

    let stored = publisher
        .store()
        .get(&String::clone(&artifact.id), now())
        .ok_or("missing")?;
    assert_eq!(
        stored.drift.as_ref().and_then(|d| d.first_seen),
        Some(artifact.ts)
    );
    assert_eq!(
        stored
            .drift
            .as_ref()
            .and_then(|d| d.occurrences)
            .map(u64::from),
        Some(1)
    );

    let mut sink = support::MemorySink(Vec::new());
    assert_eq!(publisher.drain(&mut sink, 8), 1);
    assert_eq!(sink.0.len(), 1);
    assert_eq!(publisher.pending(), 0);
    assert_eq!(publisher.drain(&mut sink, 8), 0);
    Ok(())
}

#[test]
fn a_repeated_publication_is_idempotent_and_is_not_redelivered() -> Result {
    let artifact = support::fixture()?;
    let mut publisher = publisher(8, 8, 8)?;
    publisher.publish(artifact.clone(), now())?;
    let mut sink = support::MemorySink(Vec::new());
    publisher.drain(&mut sink, 8);

    let receipt = publisher.publish(artifact.clone(), now())?;
    assert_eq!(receipt.outcome, Append::Duplicate);
    assert!(!receipt.queued);
    assert_eq!(publisher.pending(), 0);
    let stored = publisher
        .store()
        .get(&String::clone(&artifact.id), now())
        .ok_or("missing")?;
    assert_eq!(
        stored
            .drift
            .as_ref()
            .and_then(|d| d.occurrences)
            .map(u64::from),
        Some(1)
    );
    Ok(())
}

#[test]
fn a_different_body_under_one_id_is_a_conflict() -> Result {
    let artifact = support::fixture()?;
    let mut publisher = publisher(8, 8, 8)?;
    publisher.publish(artifact.clone(), now())?;
    let mut conflicting = artifact;
    conflicting.tool = "stripe.list_disputes".try_into()?;
    assert!(matches!(
        publisher.publish(conflicting, now()),
        Err(Error::Conflict)
    ));
    assert_eq!(publisher.pending(), 1);
    Ok(())
}

#[test]
fn a_second_artifact_with_the_same_drift_increments_the_count() -> Result {
    let first = support::fixture()?;
    let first_ts = first.ts;
    let mut second = first.clone();
    second.id = "aud_second".try_into()?;
    second.ts = first_ts + Duration::days(1);
    let mut publisher = publisher(8, 8, 8)?;
    publisher.publish(first, now())?;
    publisher.publish(second, now())?;

    let stored = publisher
        .store()
        .get("aud_second", now())
        .ok_or("missing")?;
    assert_eq!(
        stored.drift.as_ref().and_then(|d| d.first_seen),
        Some(first_ts)
    );
    assert_eq!(
        stored
            .drift
            .as_ref()
            .and_then(|d| d.occurrences)
            .map(u64::from),
        Some(2)
    );
    Ok(())
}

#[test]
fn every_decision_is_publishable_including_bypasses_and_clean_calls() -> Result {
    let mut publisher = publisher(16, 16, 16)?;
    let mut artifacts = vec![
        support::clean_fixture()?,
        support::bypassed()?,
        support::blocked()?,
    ];
    for decision in [
        AuditArtifactDecision::Ok,
        AuditArtifactDecision::Degraded,
        AuditArtifactDecision::Repaired,
    ] {
        let mut artifact = support::fixture()?;
        artifact.id = format!("aud_{decision}").try_into()?;
        artifact.decision = decision;
        if decision != AuditArtifactDecision::Repaired {
            artifact.patch = None;
        }
        artifacts.push(artifact);
    }
    for artifact in artifacts {
        let receipt = publisher.publish(artifact, now())?;
        assert_eq!(receipt.outcome, Append::Stored);
    }
    assert_eq!(publisher.store().len(), 6);
    Ok(())
}

#[test]
fn outbox_exhaustion_drops_delivery_not_storage() -> Result {
    let first = support::fixture()?;
    let mut second = first.clone();
    second.id = "aud_second".try_into()?;
    let mut publisher = publisher(8, 8, 1)?;
    publisher.publish(first, now())?;
    let receipt = publisher.publish(second, now())?;
    assert_eq!(receipt.outcome, Append::Stored);
    assert!(!receipt.queued);
    assert_eq!(publisher.dropped(), 1);
    assert_eq!(publisher.pending(), 1);
    assert!(publisher.store().get("aud_second", now()).is_some());
    Ok(())
}

#[test]
fn recurrence_exhaustion_leaves_the_artifact_unstamped_but_published() -> Result {
    let first = support::fixture()?;
    let mut second = first.clone();
    second.id = "aud_second".try_into()?;
    {
        let drift = second.drift.as_mut().ok_or("fixture has drift")?;
        // The fixture ships pre-stamped; start from unstamped evidence.
        drift.first_seen = None;
        drift.occurrences = None;
        // A different signature, so a full map has to refuse the new entry.
        drift
            .changes
            .first_mut()
            .ok_or("fixture has a change")?
            .path = "/different".try_into()?;
    }
    let mut publisher = publisher(8, 1, 8)?;
    publisher.publish(first, now())?;
    publisher.publish(second, now())?;
    assert_eq!(publisher.recurrence_dropped(), 1);
    let stored = publisher
        .store()
        .get("aud_second", now())
        .ok_or("missing")?;
    assert_eq!(stored.drift.as_ref().and_then(|d| d.occurrences), None);
    assert_eq!(stored.drift.as_ref().and_then(|d| d.first_seen), None);
    Ok(())
}

#[test]
fn a_failing_sink_keeps_the_event_pending_and_never_throws() -> Result {
    let artifact = support::fixture()?;
    let mut publisher = publisher(8, 8, 8)?;
    publisher.publish(artifact, now())?;
    assert_eq!(publisher.drain(&mut support::FailingSink, 8), 0);
    assert_eq!(publisher.pending(), 1);
    let mut sink = support::MemorySink(Vec::new());
    assert_eq!(publisher.drain(&mut sink, 8), 1);
    assert_eq!(sink.0.len(), 1);
    Ok(())
}

#[test]
fn store_refusal_is_reported_without_queueing() -> Result {
    let first = support::fixture()?;
    let mut second = first.clone();
    second.id = "aud_second".try_into()?;
    let mut publisher = publisher(1, 8, 8)?;
    publisher.publish(first, now())?;
    assert!(matches!(
        publisher.publish(second, now()),
        Err(Error::Capacity)
    ));
    assert_eq!(publisher.pending(), 1);
    assert_eq!(publisher.store().len(), 1);
    Ok(())
}

#[test]
fn stale_publications_are_refused() -> Result {
    let mut artifact = support::fixture()?;
    artifact.ts = now() - Duration::hours(2);
    let mut publisher = publisher(8, 8, 8)?;
    assert!(matches!(
        publisher.publish(artifact, now()),
        Err(Error::Expired)
    ));
    assert_eq!(publisher.pending(), 0);
    Ok(())
}

#[test]
fn ndjson_sink_streams_one_artifact_per_line() -> Result {
    let artifact = support::fixture()?;
    let mut output = Vec::new();
    {
        let mut sink = NdjsonSink::new(&mut output);
        sink.emit(&artifact)?;
        sink.emit(&artifact)?;
    }
    let lines: Vec<&str> = std::str::from_utf8(&output)?.lines().collect();
    assert_eq!(lines.len(), 2);
    let decoded: agw_audit::contracts::audit_artifact::AuditArtifact =
        serde_json::from_str(lines[0])?;
    assert_eq!(String::clone(&decoded.id), String::clone(&artifact.id));
    Ok(())
}
