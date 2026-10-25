//! TI-47: bounds, publication and shared-fixture validation for composed audits.
#[path = "support/audit.rs"]
mod audit;
#[path = "support/statistical.rs"]
mod support;
use agw_audit::contracts::audit_artifact::{AuditArtifact, AuditArtifactDecision as Verdict};
use agw_audit::publish::{Publisher, PublisherConfig};
use agw_diff::{Change, DriftClass, Kind, Report, Side};
use agw_guard::statistical::{Engine, Operation, Plan};
use audit::{Offline, draft, statistics};
use chrono::{TimeZone, Utc};
use support::{body, context, field, identity};
type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

#[test]
fn evidence_bounds_are_explicit_and_truncation_is_reported() -> Result {
    let mut changes = Vec::new();
    for index in 0..1100 {
        changes.push(Change {
            side: Side::Request,
            path: format!("/f{index}"),
            to_path: None,
            kind: Kind::Removed,
            class: DriftClass::Shape,
            reason: agw_diff::Reason::DeclaredStructure,
        });
    }
    let report = Report {
        changes,
        skipped: None,
    };
    let engine = Engine::statistical(Plan::new(identity(), Vec::new())?);
    let output = engine
        .begin_call(
            context("observe")?,
            Operation::Read,
            "audit",
            1_000_000_000,
            0,
        )
        .execute_with(
            agw_guard::statistical::Body {
                bytes: b"{}",
                truncated: false,
                at: Utc.timestamp_opt(128, 0).single().ok_or("at")?,
                findings: Some(&report),
            },
            None,
            &agw_guard::statistical::Builtins,
            || ("original", body(b"{}")),
        );
    let audit = output.compose(draft()?);
    let drift = audit.drift.as_ref().ok_or("drift")?;
    assert_eq!(drift.changes.len(), 1024);
    assert_eq!(statistics(&audit)?["truncated"]["changes"], 76);
    assert_eq!(statistics(&audit)?["limits"]["changes"], 1024);
    assert_eq!(
        statistics(&audit)?["truncated"]["finding_bytes"].as_bool(),
        Some(false)
    );
    Ok(())
}

#[test]
fn publication_is_optional_local_and_never_replaces_the_original_outcome() -> Result {
    let engine = Engine::statistical(Plan::new(
        identity(),
        vec![field(Side::Request, "1", "100")?],
    )?);
    let output = engine
        .begin_call(
            context("observe")?,
            Operation::Read,
            "audit",
            100_000_000,
            0,
        )
        .execute(body(b"{\"value\":100}"), None, || ("original", body(b"{}")));
    let mut publisher = Publisher::new(
        "tenant",
        PublisherConfig {
            outbox_capacity: 1,
            ..PublisherConfig::default()
        },
    )?;
    let now = Utc.timestamp_opt(200, 0).single().ok_or("now")?;
    let first = output
        .publish(draft()?, &mut publisher, now)
        .ok_or("first publication")?;
    assert!(first.queued);
    let mut second_draft = draft()?;
    second_draft.id = "aud_second".try_into()?;
    let second = output
        .publish(second_draft, &mut publisher, now)
        .ok_or("second publication")?;
    assert!(!second.queued, "a full outbox drops delivery, not storage");
    assert_eq!(publisher.dropped(), 1);
    assert_eq!(publisher.pending(), 1);
    let mut sink = Offline;
    assert_eq!(
        publisher.drain(&mut sink, 8),
        0,
        "a failing sink never blocks the call"
    );
    assert_eq!(publisher.pending(), 1);
    assert_eq!(
        output.outcome,
        Some("original"),
        "publication cannot replace the outcome"
    );
    Ok(())
}

#[test]
fn bypassed_calls_publish_like_any_other_artifact() -> Result {
    let engine = Engine::statistical(Plan::new(
        identity(),
        vec![field(Side::Request, "1", "100")?],
    )?);
    engine.set_ready(false);
    let output = engine
        .begin_call(
            context("strict")?,
            Operation::Write,
            "audit",
            100_000_000,
            0,
        )
        .execute(body(b"{\"value\":100}"), None, || ("original", body(b"{}")));
    assert_eq!(output.bypasses, 1, "the fixture must actually bypass");
    let mut publisher = Publisher::new("tenant", PublisherConfig::default())?;
    let now = Utc.timestamp_opt(200, 0).single().ok_or("now")?;
    let mut bypass = draft()?;
    bypass.id = "aud_bypass".try_into()?;
    let publication = output
        .publish(bypass, &mut publisher, now)
        .ok_or("bypass publication")?;
    let stored = publisher
        .store()
        .get(&publication.id, now)
        .ok_or("stored bypass")?;
    assert_eq!(
        stored.decision,
        agw_audit::contracts::audit_artifact::AuditArtifactDecision::Bypassed
    );
    Ok(())
}

#[test]
fn composed_artifact_round_trips_through_the_frozen_contract_fixture() -> Result {
    let artifact: AuditArtifact = serde_json::from_str(include_str!(
        "../../../../../contracts/examples/audit_artifact.statistical.valid.json"
    ))?;
    let stats = statistics(&artifact)?;
    assert_eq!(stats["schema"], "agindow_statistical/1");
    assert_eq!(artifact.decision, Verdict::Blocked);
    Ok(())
}
