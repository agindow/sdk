//! Durable bounded delivery telemetry, rendered without network I/O.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeliveryMetrics {
    pub deliveries: u64,
    pub retries: u64,
    pub drops: u64,
    pub rejected: u64,
    pub conflicts: u64,
    pub drains: u64,
    pub duration_ns: u64,
}
impl DeliveryMetrics {
    pub(super) fn reserved() -> Self {
        Self {
            deliveries: u64::MAX,
            retries: u64::MAX,
            drops: u64::MAX,
            rejected: u64::MAX,
            conflicts: u64::MAX,
            drains: u64::MAX,
            duration_ns: u64::MAX,
        }
    }
    /// Cumulative OTLP sums; durations are measured off-path, in nanoseconds.
    pub fn export_json(&self, now: DateTime<Utc>) -> Value {
        let time = now.timestamp_nanos_opt().unwrap_or(0).to_string();
        let metrics: Vec<Value> = [
            ("audit_deliveries_total", self.deliveries, "{artifact}"),
            ("audit_delivery_retries_total", self.retries, "{attempt}"),
            ("audit_delivery_drops_total", self.drops, "{artifact}"),
            ("audit_delivery_rejected_total", self.rejected, "{artifact}"),
            (
                "audit_delivery_conflicts_total",
                self.conflicts,
                "{artifact}",
            ),
            ("audit_drains_total", self.drains, "{drain}"),
            ("audit_drain_duration", self.duration_ns, "ns"),
        ]
        .into_iter()
        .map(|(name, value, unit)| {
            json!({
                "name": name, "unit": unit,
                "sum": { "aggregationTemporality": 2, "isMonotonic": true,
                    "dataPoints": [{ "timeUnixNano": time, "asInt": value.to_string() }] }
            })
        })
        .collect();
        json!({ "resourceMetrics": [{
            "resource": { "attributes": [{ "key": "service.name", "value": { "stringValue": "agindow-relay" } }] },
            "scopeMetrics": [{ "scope": { "name": "agindow.audit.delivery", "version": "1" }, "metrics": metrics }]
        }] })
    }
}
