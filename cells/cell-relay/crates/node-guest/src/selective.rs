//! Preparation only. Host verification is required before any SDK applies bytes.
use agw_diff::{Change, DriftClass, Kind, Reason, Report, Side};
use agw_repair::{Candidate, Evidence, Mode, Operation};
use serde_json::json;

pub fn prepare(
    bytes: &[u8],
    source: &str,
    target: &str,
    confidence: f64,
    mode: &str,
    work: u32,
) -> Result<String, String> {
    let key = |value: &str| {
        !value.is_empty()
            && value.len() <= 64
            && value
                .bytes()
                .enumerate()
                .all(|(i, c)| c.is_ascii_alphabetic() || c == b'_' || i > 0 && c.is_ascii_digit())
    };
    if !key(source)
        || !key(target)
        || source == target
        || !confidence.is_finite()
        || !(0.0..=1.0).contains(&confidence)
    {
        return Err("invalid reviewed rename".into());
    }
    if bytes.len() > 1_048_576 || bytes.len().saturating_mul(16) > work as usize {
        return Err("repair inspection budget exhausted".into());
    }
    let mode = match mode {
        "observe" => Mode::Observe,
        "repair" => Mode::Repair,
        "strict" => Mode::Strict,
        _ => return Err("invalid mode".into()),
    };
    let inspection = agw_inspector::inspect(bytes, false, None);
    let entries = match &inspection.parse {
        agw_inspector::Parse::Parsed(agw_inspector::Value::Object(entries)) => entries,
        agw_inspector::Parse::Parsed(_) => {
            return Ok(json!({"matched":false,"applied":false}).to_string());
        }
        _ => return Err("repair inspection unavailable".into()),
    };
    if !entries.iter().any(|(key, _)| key == source) {
        return Ok(json!({"matched":false,"applied":false}).to_string());
    }
    let report = Report {
        changes: vec![Change {
            side: Side::Request,
            path: source.into(),
            to_path: Some(target.into()),
            kind: Kind::Renamed,
            class: DriftClass::Shape,
            reason: Reason::ConfirmedIdentity,
        }],
        skipped: None,
    };
    let candidate = Candidate {
        finding: 0,
        operation: Operation::Rename { to: target.into() },
        evidence: Evidence {
            detector: confidence,
            applicability: confidence,
        },
    };
    let outcome = agw_repair::repair(
        Side::Request,
        bytes,
        &inspection,
        &report,
        &[candidate],
        mode,
    );
    if outcome.failure.is_some() {
        return Err("repair inspection unavailable".into());
    }
    let applied = outcome.patch().is_some_and(|patch| patch.applied);
    Ok(json!({"matched":true,"applied":applied,
        "body":if applied { Some(std::str::from_utf8(&outcome.body).map_err(|_| "invalid repaired UTF-8")?) } else { None },
        "confidence":outcome.confidence(),
        "halt": agw_policy::decide(mode, agw_policy::Operation::Unknown, Some(DriftClass::Shape), agw_policy::Repair::Unavailable) == agw_policy::Decision::Halt}).to_string())
}
