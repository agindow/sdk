//! Bounded operator inputs and immutable receipt/snapshot values.
use crate::credential::{Cause, Evidence};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Idle,
    Canary,
    Promoted,
    Overlap,
    Aborted,
    Unsupported,
    Reverted,
    Retired,
    Unknown,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Read,
    Write,
    Unknown,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Probe {
    pub operation: String,
    pub kind: Kind,
    pub write_opt_in: bool,
    /// Only presence is retained. The host passes the actual key straight to upstream.
    pub idempotency_key_present: bool,
    pub status: u16,
    pub latency_ms: u64,
    pub audit_ref: String,
    pub cause: Option<Cause>,
    pub evidence: Evidence,
}
#[derive(Debug, Clone, Copy)]
pub struct Config {
    pub dual_key: Option<bool>,
    pub max_probes: usize,
    pub deadline: u64,
    pub overlap_ms: u64,
    pub threshold: u32,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            dual_key: None,
            max_probes: 16,
            deadline: 30000,
            overlap_ms: 300000,
            threshold: 3,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    pub unsupported_reason: Option<UnsupportedReason>,
    pub id: String,
    pub phase: Phase,
    pub probes: Vec<Probe>,
    pub window_start: Option<u64>,
    pub window_end: Option<u64>,
    pub failing_operation: Option<String>,
    pub audit_ref: Option<String>,
    pub failure_count: u32,
    /// Audit marker: probe results are never submitted to a guard/statistical session.
    pub canary: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Definition {
    pub operation: String,
    pub kind: Kind,
    pub write_opt_in: bool,
    pub idempotency_key_present: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnsupportedReason {
    AtomicInvalidation,
    UnknownCapability,
}
