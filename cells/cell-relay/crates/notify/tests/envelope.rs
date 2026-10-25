//! TII-28 envelope fixtures and consolidation identity.
mod support;
use agw_audit::contracts::notification::{
    ConsolidationMode, FindingClass, FindingSide, Notification, NotificationSeverity,
};
use agw_notify::envelope::{self, Alert};
use support::at;
type Result = std::result::Result<(), Box<dyn std::error::Error>>;

const REFS: &[&str] = &["aud_review_1"];

fn alert() -> Alert<'static> {
    Alert {
        tenant: "acme",
        connection: "conn_1",
        vendor: "stripe",
        operation: "create_customer",
        class: FindingClass::Shape,
        side: FindingSide::Request,
        severity: NotificationSeverity::Warning,
        occurrences: 3,
        first_seen: at("2026-10-21T09:00:00Z"),
        last_seen: at("2026-10-21T11:59:00Z"),
        audit_refs: REFS,
        day: 20_747,
        ordinal: 2,
        mode: ConsolidationMode::Individual,
        consolidated: 1,
        ts: at("2026-10-21T12:00:00Z"),
    }
}

fn build_with_refs(references: &[&str]) -> Result {
    let mut value = alert();
    value.audit_refs = references;
    let _ = envelope::build(value)?;
    Ok(())
}

#[test]
fn valid_fixture_round_trips_in_rust() -> Result {
    let raw = include_str!("../../../../../contracts/examples/notification.valid.json");
    let value: Notification = serde_json::from_str(raw)?;
    envelope::validate(&value)?;
    let emitted = serde_json::to_string(&value)?;
    let replayed: serde_json::Value = serde_json::from_str(&emitted)?;
    let original: serde_json::Value = serde_json::from_str(raw)?;
    assert_eq!(replayed, original);
    Ok(())
}

#[test]
fn invalid_fixture_is_rejected_structurally() {
    let raw = include_str!("../../../../../contracts/examples/notification.invalid.json");
    assert!(serde_json::from_str::<Notification>(raw).is_err());
}

#[test]
fn identity_is_stable_per_alert_and_reused_by_summary_updates() -> Result {
    let first = envelope::build(alert())?;
    let again = envelope::build(alert())?;
    assert_eq!(first.id, again.id);
    assert!(first.id.as_str().starts_with("ntf_"));
    assert_eq!(first.id.as_str().len(), 36);

    let mut other_day = alert();
    other_day.day = 20_748;
    assert_ne!(envelope::build(other_day)?.id, first.id);
    let mut other_connection = alert();
    other_connection.connection = "conn_2";
    assert_ne!(envelope::build(other_connection)?.id, first.id);

    let mut summary = alert();
    summary.mode = ConsolidationMode::Summary;
    summary.ordinal = 5;
    summary.consolidated = 4;
    summary.occurrences = 9;
    let mut retained = envelope::build(summary)?;
    let identity = retained.id.clone();
    envelope::update_summary(
        &mut retained,
        12,
        at("2026-10-21T12:30:00Z"),
        at("2026-10-21T12:30:00Z"),
        6,
    )?;
    assert_eq!(retained.id, identity);
    assert_eq!(retained.occurrences.count.get(), 12);
    assert_eq!(retained.consolidation.count.get(), 6);
    assert_eq!(retained.ts, at("2026-10-21T12:30:00Z"));
    Ok(())
}

#[test]
fn summary_updates_refuse_individuals_and_regressions() -> Result {
    let mut individual = envelope::build(alert())?;
    assert!(
        envelope::update_summary(
            &mut individual,
            4,
            at("2026-10-21T12:30:00Z"),
            at("2026-10-21T12:30:00Z"),
            1,
        )
        .is_err()
    );

    let mut summary = alert();
    summary.mode = ConsolidationMode::Summary;
    summary.ordinal = 5;
    summary.occurrences = 9;
    summary.consolidated = 4;
    let mut retained = envelope::build(summary)?;

    assert!(
        envelope::update_summary(
            &mut retained,
            8,
            at("2026-10-21T12:30:00Z"),
            at("2026-10-21T12:30:00Z"),
            4,
        )
        .is_err()
    );
    assert!(
        envelope::update_summary(
            &mut retained,
            10,
            at("2026-10-21T12:30:00Z"),
            at("2026-10-21T12:30:00Z"),
            3,
        )
        .is_err()
    );
    assert!(
        envelope::update_summary(
            &mut retained,
            10,
            at("2026-10-21T11:00:00Z"),
            at("2026-10-21T12:30:00Z"),
            4,
        )
        .is_err()
    );
    assert!(
        envelope::update_summary(
            &mut retained,
            10,
            at("2026-10-21T12:30:00Z"),
            at("2026-10-21T11:30:00Z"),
            4,
        )
        .is_err()
    );
    assert_eq!(retained.occurrences.count.get(), 9);
    Ok(())
}

#[test]
fn build_rejects_unbounded_identifiers_and_secrets() {
    let mut control = alert();
    control.tenant = "bad\ntenant";
    assert!(envelope::build(control).is_err());

    let long = "x".repeat(257);
    let mut oversized = alert();
    oversized.operation = long.as_str();
    assert!(envelope::build(oversized).is_err());

    let mut secret = alert();
    secret.connection = "sk_live_51Nxxxxxxxxxxxxxxxxxxxx";
    assert!(envelope::build(secret).is_err());
}

#[test]
fn individual_alerts_require_evidence_and_never_consolidate() {
    let mut empty = alert();
    empty.audit_refs = &[];
    assert!(envelope::build(empty).is_err());
    let mut two = alert();
    two.consolidated = 2;
    assert!(envelope::build(two).is_err());
}

#[test]
fn bounds_identifiers_counts_references_and_total_size() {
    let long_ref = "x".repeat(257);
    assert!(build_with_refs(&[long_ref.as_str()]).is_err());
    let names: Vec<String> = (0..33).map(|index| format!("aud_{index}")).collect();
    let borrowed: Vec<&str> = names.iter().map(String::as_str).collect();
    assert!(build_with_refs(&borrowed).is_err());
    let wide: Vec<String> = (0..32).map(|index| format!("{index:0>256}")).collect();
    let wide_refs: Vec<&str> = wide.iter().map(String::as_str).collect();
    assert!(build_with_refs(&wide_refs).is_err());
    let mut too_many = alert();
    too_many.occurrences = 1_000_001;
    assert!(envelope::build(too_many).is_err());
}

#[test]
fn credential_shapes_and_payload_bytes_never_become_references() {
    let hex = "dead".repeat(32);
    let mut cases = vec![
        "Bearer sk_live_51Nxxxxxxxxxxxxxxxx".to_string(),
        "sk_live_51Nxxxxxxxxxxxxxxxxxxxx".to_string(),
        "AKIAIOSFODNN7EXAMPLE".to_string(),
        "ghp_1234567890abcdefghijklmnopqrstuvwxyz".to_string(),
        "-----BEGIN PRIVATE KEY-----".to_string(),
        "password=hunter2".to_string(),
        "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJhY21lIn0.c2lnbmF0dXJl".to_string(),
        hex,
        "line\nbreak".to_string(),
    ];
    for reference in &mut cases {
        assert!(
            build_with_refs(&[reference.as_str()]).is_err(),
            "{reference}"
        );
    }
}

#[test]
fn times_must_be_ordered_and_bounded_by_emission() {
    let mut future = alert();
    future.last_seen = at("2026-10-21T13:00:00Z");
    assert!(envelope::build(future).is_err());
    let mut backwards = alert();
    backwards.first_seen = at("2026-10-21T11:00:00Z");
    backwards.last_seen = at("2026-10-21T10:00:00Z");
    assert!(envelope::build(backwards).is_err());
}
