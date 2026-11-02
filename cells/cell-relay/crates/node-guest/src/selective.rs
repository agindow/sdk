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
    bounded(bytes, confidence, work)?;
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
    prepare_paths(
        bytes,
        &inspection,
        vec![source.into()],
        target,
        confidence,
        mode_string(mode),
    )
}

fn mode_string(mode: Mode) -> &'static str {
    match mode {
        Mode::Observe => "observe",
        Mode::Repair => "repair",
        Mode::Strict => "strict",
    }
}

pub(crate) fn bounded(bytes: &[u8], confidence: f64, work: u32) -> Result<(), String> {
    if !confidence.is_finite()
        || !(0.0..=1.0).contains(&confidence)
        || bytes.len() > 1_048_576
        || bytes.len().saturating_mul(16) > work as usize
    {
        return Err("repair inspection budget exhausted".into());
    }
    Ok(())
}

pub(crate) fn prepare_paths(
    bytes: &[u8],
    inspection: &agw_inspector::Inspection,
    paths: Vec<String>,
    target: &str,
    confidence: f64,
    mode: &str,
) -> Result<String, String> {
    let mode = match mode {
        "observe" => Mode::Observe,
        "repair" => Mode::Repair,
        "strict" => Mode::Strict,
        _ => return Err("invalid mode".into()),
    };
    let changes: Vec<_> = paths
        .iter()
        .map(|path| {
            let mut escaped = false;
            let mut last_dot = None;
            for (i, c) in path.char_indices() {
                if escaped {
                    escaped = false;
                } else if c == '\\' {
                    escaped = true;
                } else if c == '.' {
                    last_dot = Some(i);
                }
            }
            let target_path = if target.is_empty() {
                "\\e".into()
            } else {
                target
                    .chars()
                    .flat_map(|c| {
                        if matches!(c, '\\' | '.' | '[' | ']' | '$') {
                            vec!['\\', c]
                        } else {
                            vec![c]
                        }
                    })
                    .collect::<String>()
            };
            let to = last_dot.map_or(target_path.clone(), |i| {
                format!("{}.{}", &path[..i], target_path)
            });
            Change {
                side: Side::Request,
                path: path.clone(),
                to_path: Some(to),
                kind: Kind::Renamed,
                class: DriftClass::Shape,
                reason: Reason::ConfirmedIdentity,
            }
        })
        .collect();
    let candidates: Vec<_> = changes
        .iter()
        .enumerate()
        .map(|(finding, _)| Candidate {
            finding,
            operation: Operation::Rename { to: target.into() },
            evidence: Evidence {
                detector: confidence,
                applicability: confidence,
            },
        })
        .collect();
    let report = Report {
        changes,
        skipped: None,
    };
    let outcome = agw_repair::repair(Side::Request, bytes, inspection, &report, &candidates, mode);
    if outcome.failure.is_some() {
        return Err("repair inspection unavailable".into());
    }
    let applied = outcome.patch().is_some_and(|patch| patch.applied);
    let changes: Vec<_> = report
        .changes
        .iter()
        .map(|change| json!({"path":change.path,"to":change.to_path}))
        .collect();
    Ok(json!({"matched":true,"applied":applied,"changes":changes,
        "body":if applied { Some(std::str::from_utf8(&outcome.body).map_err(|_| "invalid repaired UTF-8")?) } else { None },
        "confidence":outcome.confidence(),
        "halt": agw_policy::decide(mode, agw_policy::Operation::Unknown, Some(DriftClass::Shape), agw_policy::Repair::Unavailable) == agw_policy::Decision::Halt}).to_string())
}
