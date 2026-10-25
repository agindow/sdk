use super::{
    ARMS, CLASSES, Outcome,
    model::{Case, Fixture, Label, Metrics},
};
use serde_json::Value;

fn json_type(value: &Value) -> &str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}
pub fn accepted(f: &Fixture, body: &Value) -> bool {
    let vendor = &f.oracle.vendor;
    body.as_object()
        .is_some_and(|o| vendor.required.iter().all(|key| o.contains_key(key)))
        && body.get(&f.task.target).is_some_and(|v| {
            vendor.target_types.iter().any(|t| t == json_type(v))
                && vendor
                    .enum_values
                    .as_ref()
                    .is_none_or(|allowed| allowed.contains(v))
        })
}
pub fn score(f: &Fixture, arm: &str, sent: Option<Value>, trace: Vec<String>) -> Case {
    let outcome = match &sent {
        None => Outcome::Blocked,
        Some(body) if !accepted(f, body) => Outcome::Rejected,
        Some(body) if *body == f.oracle.expected => Outcome::SafeWrite,
        Some(_) => Outcome::UnsafeWrite,
    };
    let recovered = outcome == Outcome::SafeWrite;
    let target_recovered = matches!(outcome, Outcome::SafeWrite | Outcome::UnsafeWrite)
        && sent
            .as_ref()
            .is_some_and(|v| v.get(&f.task.target) == f.oracle.expected.get(&f.task.target));
    let successful_repair = recovered && sent.as_ref().is_some_and(|v| *v != f.task.input);
    Case {
        id: f.id.clone(),
        class: f.class.clone(),
        arm: arm.into(),
        label: f.oracle.label,
        outcome,
        recovered,
        target_recovered,
        successful_repair,
        violation: outcome == Outcome::UnsafeWrite,
        sent,
        trace,
    }
}

pub(super) fn metric(cases: &[Case], class: &str, label: &str, arm: &str) -> Metrics {
    let group: Vec<_> = cases
        .iter()
        .filter(|c| {
            c.arm == arm
                && (class == "all" || c.class == class)
                && (label == "all" || c.label.name() == label)
        })
        .collect();
    let total = group.len();
    let count = |outcome| group.iter().filter(|c| c.outcome == outcome).count();
    let safe_writes = count(Outcome::SafeWrite);
    let unsafe_writes = count(Outcome::UnsafeWrite);
    let blocked = count(Outcome::Blocked);
    let repairable_total = group
        .iter()
        .filter(|c| c.label == Label::Repairable)
        .count();
    let repairable_recovered = group
        .iter()
        .filter(|c| c.label == Label::Repairable && c.recovered)
        .count();
    Metrics {
        class: class.into(),
        label: label.into(),
        arm: arm.into(),
        total,
        safe_writes,
        unsafe_writes,
        blocked,
        rejected: count(Outcome::Rejected),
        successful_repairs: group.iter().filter(|c| c.successful_repair).count(),
        target_recovered: group.iter().filter(|c| c.target_recovered).count(),
        repairable_total,
        repairable_recovered,
        repairable_missed: repairable_total - repairable_recovered,
        recovery_rate: safe_writes as f64 / total as f64,
        repairable_recovery_rate: (repairable_total > 0)
            .then(|| repairable_recovered as f64 / repairable_total as f64),
        violation_rate: unsafe_writes as f64 / total as f64,
        violations_per_dispatch: (total > blocked)
            .then(|| unsafe_writes as f64 / (total - blocked) as f64),
    }
}
pub fn metrics(cases: &[Case]) -> Vec<Metrics> {
    let mut result = Vec::new();
    for arm in ARMS {
        for class in CLASSES.into_iter().chain(["all"]) {
            result.push(metric(cases, class, "all", arm));
        }
        for label in [Label::Repairable, Label::Ambiguous, Label::Unsupported] {
            result.push(metric(cases, "all", label.name(), arm));
        }
    }
    result
}
