//! TI-29 OTel projection: TECH_STACK §3.8 counters and a caller-supplied p99.
mod support;

use agw_audit::Error;
use agw_audit::contracts::audit_artifact::{DriftClass, DriftSide};
use agw_audit::otel::Metrics;
use chrono::{TimeZone, Utc};
use serde_json::Value;
use std::error::Error as StdError;

type Result = std::result::Result<(), Box<dyn StdError>>;

fn now() -> chrono::DateTime<Utc> {
    const SECONDS: i64 = 1_761_115_211;
    Utc.timestamp_opt(SECONDS, 0)
        .single()
        .unwrap_or_else(Utc::now)
}

fn metric<'a>(document: &'a Value, name: &str) -> Option<&'a Value> {
    document["resourceMetrics"][0]["scopeMetrics"][0]["metrics"]
        .as_array()?
        .iter()
        .find(|entry| entry["name"] == name)
}

fn point<'a>(document: &'a Value, name: &str, attributes: &[(&str, &str)]) -> Option<&'a Value> {
    metric(document, name)?["sum"]["dataPoints"]
        .as_array()?
        .iter()
        .find(|point| {
            attributes.iter().all(|(key, value)| {
                point["attributes"].as_array().is_some_and(|list| {
                    list.iter().any(|attribute| {
                        attribute["key"] == *key && attribute["value"]["stringValue"] == *value
                    })
                })
            })
        })
}

#[test]
fn export_carries_every_contract_metric_name() -> Result {
    let mut metrics = Metrics::default();
    metrics.set_guard_latency_p99(12.5)?;
    let document = metrics.export_json(now());
    for name in [
        "drift_events_total",
        "repairs_total",
        "policy_violations_total",
        "guard_latency_p99",
        "bypassed_total",
        "blocked_writes_total",
    ] {
        assert!(metric(&document, name).is_some(), "missing metric {name}");
    }
    assert_eq!(
        metric(&document, "guard_latency_p99").ok_or("guard_latency_p99")?["gauge"]["dataPoints"]
            [0]["asDouble"],
        12.5
    );
    Ok(())
}

#[test]
fn drift_events_are_labelled_by_class_and_side() -> Result {
    let mut metrics = Metrics::default();
    metrics.record(&support::fixture()?);
    let mut response = support::fixture()?;
    response.id = "aud_response".try_into()?;
    let drift = response.drift.as_mut().ok_or("missing drift")?;
    drift.class = DriftClass::Semantic;
    drift.side = DriftSide::Response;
    response.patch = None;
    response.decision = agw_audit::contracts::audit_artifact::AuditArtifactDecision::Degraded;
    metrics.record(&response);

    let document = metrics.export_json(now());
    let shape = point(
        &document,
        "drift_events_total",
        &[("class", "shape"), ("side", "request")],
    )
    .ok_or("missing shape point")?;
    assert_eq!(shape["asInt"], "1");
    let semantic = point(
        &document,
        "drift_events_total",
        &[("class", "semantic"), ("side", "response")],
    )
    .ok_or("missing semantic point")?;
    assert_eq!(semantic["asInt"], "1");
    Ok(())
}

#[test]
fn repair_outcomes_cover_applied_skipped_and_failed() -> Result {
    let mut metrics = Metrics::default();
    metrics.record(&support::fixture()?);
    metrics.record(&support::blocked()?);
    let mut failed = support::fixture()?;
    failed.id = "aud_failed".try_into()?;
    failed.patch = None;
    failed.decision = agw_audit::contracts::audit_artifact::AuditArtifactDecision::Degraded;
    failed.detail = Some("repair_verification_failed".try_into()?);
    metrics.record(&failed);

    let document = metrics.export_json(now());
    assert_eq!(
        point(&document, "repairs_total", &[("outcome", "applied")]).ok_or("applied")?["asInt"],
        "1"
    );
    assert_eq!(
        point(&document, "repairs_total", &[("outcome", "skipped")]).ok_or("skipped")?["asInt"],
        "1"
    );
    assert_eq!(
        point(&document, "repairs_total", &[("outcome", "failed")]).ok_or("failed")?["asInt"],
        "1"
    );
    Ok(())
}

#[test]
fn policy_violations_bypassed_and_blocked_writes_count_independently() -> Result {
    let mut metrics = Metrics::default();
    metrics.record(&support::blocked()?);
    metrics.record(&support::bypassed()?);
    metrics.record(&support::clean_fixture()?);

    let document = metrics.export_json(now());
    assert_eq!(
        metric(&document, "policy_violations_total").ok_or("policy_violations_total")?["sum"]["dataPoints"]
            [0]["asInt"],
        "1"
    );
    assert_eq!(
        metric(&document, "blocked_writes_total").ok_or("blocked_writes_total")?["sum"]["dataPoints"]
            [0]["asInt"],
        "1"
    );
    assert_eq!(
        metric(&document, "bypassed_total").ok_or("bypassed_total")?["sum"]["dataPoints"][0]["asInt"],
        "1"
    );
    Ok(())
}

#[test]
fn latency_gauge_rejects_non_finite_or_negative_values() {
    let mut metrics = Metrics::default();
    assert!(matches!(
        metrics.set_guard_latency_p99(f64::NAN),
        Err(Error::Invalid(_))
    ));
    assert!(matches!(
        metrics.set_guard_latency_p99(-1.0),
        Err(Error::Invalid(_))
    ));
}

#[test]
fn export_is_deterministic_for_the_same_state() -> Result {
    let mut metrics = Metrics::default();
    metrics.record(&support::fixture()?);
    metrics.record(&support::bypassed()?);
    assert_eq!(metrics.export_json(now()), metrics.export_json(now()));
    Ok(())
}
