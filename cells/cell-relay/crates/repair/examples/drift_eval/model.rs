use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Label {
    Repairable,
    Ambiguous,
    Unsupported,
}
impl Label {
    pub fn name(self) -> &'static str {
        match self {
            Self::Repairable => "repairable",
            Self::Ambiguous => "ambiguous",
            Self::Unsupported => "unsupported",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    SafeWrite,
    UnsafeWrite,
    Blocked,
    Rejected,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Proposal {
    pub value: Value,
    pub confidence: f64,
    pub rename_to: Option<String>,
}
/// This is the entire input visible to adapters. Oracle intent, labels and
/// rationales cannot be used to decide a repair or mark it verified.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Task {
    pub input: Value,
    pub target: String,
    pub semantic: bool,
    pub rename: bool,
    pub confirmed: bool,
    pub proposal: Option<Proposal>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Vendor {
    pub required: Vec<String>,
    pub target_types: Vec<String>,
    pub enum_values: Option<Vec<Value>>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Oracle {
    pub label: Label,
    pub rationale: String,
    pub expected: Value,
    pub vendor: Vendor,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fixture {
    pub id: String,
    pub class: String,
    pub task: Task,
    pub oracle: Oracle,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Case {
    pub id: String,
    pub class: String,
    pub arm: String,
    pub label: Label,
    pub outcome: Outcome,
    pub recovered: bool,
    pub target_recovered: bool,
    pub successful_repair: bool,
    pub violation: bool,
    pub sent: Option<Value>,
    pub trace: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Metrics {
    pub class: String,
    pub label: String,
    pub arm: String,
    pub total: usize,
    pub safe_writes: usize,
    pub unsafe_writes: usize,
    pub blocked: usize,
    pub rejected: usize,
    pub successful_repairs: usize,
    pub target_recovered: usize,
    pub repairable_total: usize,
    pub repairable_recovered: usize,
    pub repairable_missed: usize,
    pub recovery_rate: f64,
    pub repairable_recovery_rate: Option<f64>,
    pub violation_rate: f64,
    pub violations_per_dispatch: Option<f64>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Report {
    pub version: u32,
    pub corpus: String,
    pub cases: Vec<Case>,
    pub metrics: Vec<Metrics>,
}
