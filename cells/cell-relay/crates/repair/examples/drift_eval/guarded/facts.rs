//! Caller-side facts only. No Task, Proposal, Fixture or Oracle enters verification.
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "rule", rename_all = "snake_case", deny_unknown_fields)]
pub enum Authority {
    Scale { factor: f64 },
    Mapping { source: Value, destination: Value },
    Rename { destination: String },
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Facts {
    pub authority: Option<Authority>,
    pub preserve_other_fields: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Declaration {
    pub version: u32,
    pub cases: std::collections::BTreeMap<String, Facts>,
}
pub const DECLARATION: &str = include_str!("../verification.v1.json");

/// Compare against pre-existing migration constraints and original context.
/// A shape-valid payload without authority still refuses verification.
pub fn verify(original: &Value, facts: &Facts, patched: &[u8], applied: &[usize]) -> bool {
    if applied != [0] || !facts.preserve_other_fields {
        return false;
    }
    let Ok(Value::Object(mut actual)) = serde_json::from_slice(patched) else {
        return false;
    };
    let Some(mut context) = original.as_object().cloned() else {
        return false;
    };
    let Some(source) = context.remove("value") else {
        return false;
    };
    let (key, valid) = match &facts.authority {
        Some(Authority::Scale { factor }) => (
            "value",
            source
                .as_f64()
                .zip(actual.get("value").and_then(Value::as_f64))
                .is_some_and(|(a, b)| a * factor == b),
        ),
        Some(Authority::Mapping {
            source: from,
            destination,
        }) => (
            "value",
            source == *from && actual.get("value") == Some(destination),
        ),
        Some(Authority::Rename { destination }) => (
            destination.as_str(),
            actual.get(destination) == Some(&source),
        ),
        None => return false,
    };
    actual.remove(key);
    valid && actual == context
}
