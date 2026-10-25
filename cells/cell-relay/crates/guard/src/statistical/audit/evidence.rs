//! Typed, bounded JSON for every admitted statistical finding (TI-47).
//!
//! One entry per admitted test. `reason` is the detector's typed explanation;
//! `hypotheses` is the multi-hypothesis slot (unit conversion today), so a
//! later reader never has to parse prose or lose per-test evidence.
use super::bounds::json_len;
use crate::statistical::{Detector, Phase, Record};
use agw_diff::Side;
use agw_stats::{control, field, significance as sig, unit};
use serde_json::{Value, json};

#[derive(Debug)]
pub(super) struct Finding {
    pub(super) key: (u8, String, &'static str, String, String),
    pub(super) value: Value,
}

fn side_key(side: Side) -> u8 {
    match side {
        Side::Request => 0,
        Side::Response => 1,
    }
}

fn value_key(value: &Value) -> String {
    value.to_string()
}

fn side_name(side: Side) -> &'static str {
    match side {
        Side::Request => "request",
        Side::Response => "response",
    }
}

fn detector_name(detector: Detector) -> &'static str {
    match detector {
        Detector::Unit => "unit",
        Detector::Enumeration => "enumeration",
        Detector::Completeness => "completeness",
        Detector::TypeConsistency => "type_consistency",
        Detector::Charset => "charset",
    }
}

fn signal_name(signal: control::Signal) -> &'static str {
    match signal {
        control::Signal::MeanShift => "mean_shift",
        control::Signal::EnumNovelty => "enum_novelty",
        control::Signal::UnitHypothesis => "unit_hypothesis",
        control::Signal::Completeness => "completeness",
        control::Signal::TypeConsistency => "type_consistency",
        control::Signal::StringFormat => "string_format",
    }
}

fn rate_value(rate: &field::Rates) -> Value {
    json!({ "baseline": rate.baseline, "observed": rate.observed })
}

fn sentinel_name(sentinel: field::Sentinel) -> &'static str {
    match sentinel {
        field::Sentinel::Missing => "missing",
        field::Sentinel::Null => "null",
        field::Sentinel::Zero => "zero",
        field::Sentinel::EmptyString => "empty_string",
    }
}

fn type_name(kind: field::Type) -> &'static str {
    match kind {
        field::Type::Null => "null",
        field::Type::Boolean => "boolean",
        field::Type::Integer => "integer",
        field::Type::Number => "number",
        field::Type::String => "string",
        field::Type::Array => "array",
        field::Type::Object => "object",
    }
}

fn class_name(class: field::CharacterClass) -> &'static str {
    match class {
        field::CharacterClass::AsciiLetter => "ascii_letter",
        field::CharacterClass::AsciiDigit => "ascii_digit",
        field::CharacterClass::Whitespace => "whitespace",
        field::CharacterClass::Punctuation => "punctuation",
        field::CharacterClass::Control => "control",
        field::CharacterClass::NonAscii => "non_ascii",
        field::CharacterClass::Replacement => "replacement",
    }
}

fn format_name(format: field::Format) -> &'static str {
    match format {
        field::Format::IsoDate => "iso_date",
        field::Format::IsoTimestamp => "iso_timestamp",
        field::Format::MonthDay => "month_day",
        field::Format::Digits => "digits",
        field::Format::Unknown => "unknown",
    }
}

fn base(
    record: &Record,
    test: &sig::Test,
    decision: &control::Decision,
    reason: Value,
    hypotheses: Vec<Value>,
) -> Value {
    let score = match decision {
        control::Decision::Eligible { score } => json!(score),
        _ => Value::Null,
    };
    json!({
        "side": side_name(record.side),
        "path": record.path,
        "detector": detector_name(record.detector),
        "class": "semantic",
        "generation": record.generation,
        "signal": signal_name(test.evidence().signal),
        "samples": { "baseline": test.samples().0, "observed": test.samples().1 },
        "effect": test.evidence().effect,
        "p_value": test.evidence().p_value,
        "tests": test.evidence().tests,
        "materiality": { "decision": "eligible", "score": score },
        "reason": reason,
        "hypotheses": hypotheses,
    })
}

fn unit_finding(record: &Record, test: &sig::Test, decision: &control::Decision) -> Value {
    let crate::statistical::Evidence::Unit(report) = &record.evidence else {
        return base(
            record,
            test,
            decision,
            json!({"kind": "unavailable"}),
            Vec::new(),
        );
    };
    let (reason, hypotheses) = match report.detection() {
        unit::Detection::Drift(finding) => {
            let metrics = finding.metrics();
            let reason = json!({
                "kind": "magnitude_shift",
                "wasserstein": metrics.wasserstein,
                "normalized_wasserstein": metrics.normalized_wasserstein,
                "psi": metrics.psi,
            });
            let hypotheses = finding
                .hypothesis()
                .map(|h| {
                    vec![json!({
                        "kind": "unit_conversion",
                        "baseline_median": h.baseline_median,
                        "observed_median": h.observed_median,
                        "ratio": h.ratio,
                        "factor": h.conversion.factor(),
                        "tolerance": h.tolerance,
                        "automatic_conversion": false,
                    })]
                })
                .unwrap_or_default();
            (reason, hypotheses)
        }
        _ => (json!({"kind": "unavailable"}), Vec::new()),
    };
    base(record, test, decision, reason, hypotheses)
}

fn enumeration_finding(record: &Record, test: &sig::Test, decision: &control::Decision) -> Value {
    let crate::statistical::Evidence::Enumeration(report) = &record.evidence else {
        return base(
            record,
            test,
            decision,
            json!({"kind": "unavailable"}),
            Vec::new(),
        );
    };
    let value = match test.hypothesis() {
        sig::Hypothesis::Category(value) => value.as_str(),
        _ => "",
    };
    let reason = report
        .detection()
        .new_values()
        .iter()
        .find(|item| item.value == value)
        .map(|item| {
            json!({
                "kind": "new_value",
                "value": item.value,
                "count": item.count,
                "frequency": item.frequency,
                "window_first_seen": item.first_seen.to_rfc3339(),
                "material": item.material,
            })
        })
        .unwrap_or_else(|| json!({"kind": "new_value", "value": value}));
    base(record, test, decision, reason, Vec::new())
}

fn field_reason(report: &field::Report, hypothesis: &sig::Hypothesis) -> Value {
    for reason in report.reasons() {
        match (hypothesis, reason) {
            (sig::Hypothesis::Completeness, field::Reason::CompletenessDelta { rates }) => {
                return json!({"kind": "completeness_delta", "rates": rate_value(rates)});
            }
            (
                sig::Hypothesis::Sentinel(wanted),
                field::Reason::SentinelShift { sentinel, rates },
            ) if wanted == sentinel => {
                return json!({
                    "kind": "sentinel_shift",
                    "sentinel": sentinel_name(*sentinel),
                    "rates": rate_value(rates),
                });
            }
            (sig::Hypothesis::Type(wanted), field::Reason::TypeConsistency { kind, rates })
                if wanted == kind =>
            {
                return json!({
                    "kind": "type_consistency",
                    "type": type_name(*kind),
                    "rates": rate_value(rates),
                });
            }
            (sig::Hypothesis::Character(wanted), field::Reason::CharsetShift { class, rates })
                if wanted == class =>
            {
                return json!({
                    "kind": "charset_shift",
                    "class": class_name(*class),
                    "rates": rate_value(rates),
                });
            }
            (
                sig::Hypothesis::Format(wanted),
                field::Reason::FormatEmerged {
                    format,
                    pattern,
                    rates,
                },
            ) if wanted == format => {
                return json!({
                    "kind": "format_emerged",
                    "format": format_name(*format),
                    "pattern": pattern,
                    "rates": rate_value(rates),
                });
            }
            _ => {}
        }
    }
    json!({"kind": "unavailable"})
}

fn field_finding(record: &Record, test: &sig::Test, decision: &control::Decision) -> Value {
    let crate::statistical::Evidence::Field(report) = &record.evidence else {
        return base(
            record,
            test,
            decision,
            json!({"kind": "unavailable"}),
            Vec::new(),
        );
    };
    let reason = field_reason(report.detection(), test.hypothesis());
    base(record, test, decision, reason, Vec::new())
}

/// Every test the materiality floor admitted, with its typed reason.
pub(super) fn statistical(record: &Record) -> Vec<Finding> {
    record
        .evidence
        .tests()
        .iter()
        .enumerate()
        .filter_map(|(index, test)| {
            let decision = record.eligibility.get(index)?.as_ref()?;
            if !decision.eligible() {
                return None;
            }
            let value = match record.detector {
                Detector::Unit => unit_finding(record, test, decision),
                Detector::Enumeration => enumeration_finding(record, test, decision),
                _ => field_finding(record, test, decision),
            };
            let reason = value["reason"]["kind"].as_str().unwrap_or("unavailable");
            Some(Finding {
                key: (
                    side_key(record.side),
                    record.path.clone(),
                    detector_name(record.detector),
                    reason.to_string(),
                    // The complete value is the final tie-breaker. Length alone
                    // allowed equal-sized hypotheses to retain caller order,
                    // violating deterministic composition under permutation.
                    value_key(&value),
                ),
                value,
            })
        })
        .collect()
}

/// The single composed ordered vector, bounded by count and serialized bytes.
pub(super) fn ordered(mut entries: Vec<Finding>) -> (Vec<Value>, usize, bool) {
    entries.sort_by(|a, b| a.key.cmp(&b.key));
    let mut kept = Vec::new();
    let mut omitted = 0;
    let mut bytes = 0usize;
    let mut byte_truncated = false;
    for entry in entries {
        let size = json_len(&entry.value);
        if kept.len() >= super::bounds::MAX_FINDINGS {
            omitted += 1;
            continue;
        }
        if bytes.saturating_add(size) > super::bounds::MAX_FINDING_BYTES {
            omitted += 1;
            byte_truncated = true;
            continue;
        }
        bytes += size;
        kept.push(entry.value);
    }
    (kept, omitted, byte_truncated)
}

/// Invariant and cross-endpoint findings are admitted evidence too; route them
/// through the same ordered, bounded representation with their own detectors.
pub(super) fn auxiliary(phase: &Phase, side: Side) -> Vec<Finding> {
    let mut entries = Vec::new();
    let cross: std::collections::BTreeSet<&str> = phase
        .cross_endpoint
        .iter()
        .filter(|c| c.reason == agw_invariants::cross_endpoint::Reason::Divergence)
        .map(|c| c.id.as_str())
        .collect();
    for record in &phase.invariants {
        if record.class().is_none() || cross.contains(record.id.as_str()) {
            continue;
        }
        let kind = if record.verdict == agw_invariants::Verdict::EnumAddition {
            "enum_addition"
        } else {
            "violation"
        };
        let value = json!({
            "side": side_name(side),
            "path": record.id,
            "detector": "invariant",
            "class": "semantic",
            "reason": {
                "kind": kind,
                "actual": record.actual,
                "expected": record.expected,
            },
            "hypotheses": [],
        });
        entries.push(Finding {
            key: (
                side_key(side),
                record.id.clone(),
                "invariant",
                kind.to_string(),
                value_key(&value),
            ),
            value,
        });
    }
    for comparison in &phase.cross_endpoint {
        if comparison.reason != agw_invariants::cross_endpoint::Reason::Divergence {
            continue;
        }
        let value = json!({
            "side": side_name(side),
            "path": comparison.id,
            "detector": "cross_endpoint",
            "class": "semantic",
            "reason": {
                "kind": "divergence",
                "concept": comparison.concept,
                "unit": comparison.unit,
                "tolerance": comparison.tolerance,
                "max_age_ms": comparison.max_age_ms,
                "max_skew_ms": comparison.max_skew_ms,
            },
            "hypotheses": [],
        });
        entries.push(Finding {
            key: (
                side_key(side),
                comparison.id.clone(),
                "cross_endpoint",
                "divergence".to_string(),
                value_key(&value),
            ),
            value,
        });
    }
    entries
}
