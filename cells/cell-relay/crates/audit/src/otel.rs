//! TI-29 OTel projection (`TECH_STACK.md` §3.8).
//!
//! Counts the six product metrics off published artifacts and renders OTLP/JSON.
//! No exporter performs network I/O: a host agent scrapes or ships the document.
//! `guard_latency_p99` is supplied by the host — this module never measures the
//! call path, so a missing gauge is absent rather than invented.

use crate::Error;
use crate::contracts::audit_artifact::{
    AuditArtifact, AuditArtifactDecision as Decision, AuditArtifactOperation as Operation,
    DriftClass, DriftSide,
};
use chrono::{DateTime, Utc};
use serde_json::{Value, json};

const CLASSES: [(&str, usize); 3] = [("shape", 0), ("semantic", 1), ("unknown", 2)];
const SIDES: [(&str, usize); 3] = [("request", 0), ("response", 1), ("both", 2)];
const OUTCOMES: [&str; 3] = ["applied", "skipped", "failed"];
const VERIFICATION_FAILED: &str = "repair_verification_failed";

/// Cumulative counters plus the caller-supplied latency gauge.
#[derive(Debug, Clone, Default)]
pub struct Metrics {
    drift: [[u64; 3]; 3],
    repairs: [u64; 3],
    policy_violations: u64,
    bypassed: u64,
    blocked_writes: u64,
    latency_p99: Option<f64>,
    publication_drops: u64,
}

impl Metrics {
    /// Delivery refused by the bounded local publication outbox.
    pub(crate) fn publication_drop(&mut self) {
        self.publication_drops = self.publication_drops.saturating_add(1);
    }

    /// Project one published artifact. Repeated publication of the same
    /// artifact is filtered by the publisher before this point.
    pub fn record(&mut self, artifact: &AuditArtifact) {
        if let Some(drift) = &artifact.drift {
            let class = match drift.class {
                DriftClass::Shape => 0,
                DriftClass::Semantic => 1,
                DriftClass::Unknown => 2,
            };
            let side = match drift.side {
                DriftSide::Request => 0,
                DriftSide::Response => 1,
                DriftSide::Both => 2,
            };
            self.drift[class][side] = self.drift[class][side].saturating_add(1);
        }
        let failed = artifact
            .detail
            .as_ref()
            .is_some_and(|detail| **detail == VERIFICATION_FAILED);
        if artifact.patch.as_ref().is_some_and(|patch| patch.applied) {
            self.repairs[0] = self.repairs[0].saturating_add(1);
        } else if failed {
            self.repairs[2] = self.repairs[2].saturating_add(1);
        } else if artifact.drift.is_some() {
            self.repairs[1] = self.repairs[1].saturating_add(1);
        }
        if artifact.decision == Decision::Blocked {
            self.policy_violations = self.policy_violations.saturating_add(1);
            if artifact.operation != Operation::Read {
                self.blocked_writes = self.blocked_writes.saturating_add(1);
            }
        }
        if artifact.decision == Decision::Bypassed {
            self.bypassed = self.bypassed.saturating_add(1);
        }
    }

    /// Set the caller-measured p99 of the guard path, in milliseconds.
    pub fn set_guard_latency_p99(&mut self, milliseconds: f64) -> Result<(), Error> {
        if !milliseconds.is_finite() || milliseconds < 0.0 {
            return Err(Error::Invalid(
                "latency p99 must be a finite non-negative number",
            ));
        }
        self.latency_p99 = Some(milliseconds);
        Ok(())
    }

    /// Render an OTLP/JSON `ResourceMetrics` document. Deterministic for one
    /// counter state; zero-valued points are included so a dashboard never
    /// loses a series when traffic stops.
    pub fn export_json(&self, time: DateTime<Utc>) -> Value {
        let nanos = time.timestamp_nanos_opt().unwrap_or(0).to_string();
        let mut metrics = Vec::new();
        let mut drift_points = Vec::new();
        for (class, class_index) in CLASSES {
            for (side, side_index) in SIDES {
                drift_points.push(point(
                    &[("class", class), ("side", side)],
                    self.drift[class_index][side_index],
                    &nanos,
                ));
            }
        }
        metrics.push(sum("drift_events_total", drift_points));
        metrics.push(sum(
            "repairs_total",
            OUTCOMES
                .iter()
                .enumerate()
                .map(|(index, outcome)| point(&[("outcome", outcome)], self.repairs[index], &nanos))
                .collect(),
        ));
        metrics.push(sum(
            "policy_violations_total",
            vec![point(&[], self.policy_violations, &nanos)],
        ));
        metrics.push(sum(
            "audit_publication_drops_total",
            vec![point(&[], self.publication_drops, &nanos)],
        ));
        metrics.push(sum(
            "bypassed_total",
            vec![point(&[], self.bypassed, &nanos)],
        ));
        metrics.push(sum(
            "blocked_writes_total",
            vec![point(&[], self.blocked_writes, &nanos)],
        ));
        if let Some(latency) = self.latency_p99 {
            metrics.push(json!({
                "name": "guard_latency_p99",
                "unit": "ms",
                "gauge": { "dataPoints": [
                    { "timeUnixNano": nanos, "asDouble": latency }
                ] }
            }));
        }
        json!({
            "resourceMetrics": [{
                "resource": { "attributes": [attribute("service.name", "agindow-relay")] },
                "scopeMetrics": [{
                    "scope": { "name": "agindow.audit", "version": "1" },
                    "metrics": metrics
                }]
            }]
        })
    }
}

fn attribute(key: &str, value: &str) -> Value {
    json!({ "key": key, "value": { "stringValue": value } })
}

fn point(attributes: &[(&str, &str)], value: u64, nanos: &str) -> Value {
    json!({
        "attributes": attributes.iter().map(|(key, value)| attribute(key, value)).collect::<Vec<_>>(),
        "timeUnixNano": nanos,
        "asInt": value.to_string()
    })
}

fn sum(name: &str, data_points: Vec<Value>) -> Value {
    json!({
        "name": name,
        "unit": "1",
        "sum": {
            "dataPoints": data_points,
            "aggregationTemporality": 2,
            "isMonotonic": true
        }
    })
}
