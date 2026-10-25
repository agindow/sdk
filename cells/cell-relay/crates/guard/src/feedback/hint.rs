use agw_diff::{DriftClass, Kind, Reason, Report};

pub(super) fn build(report: &Report, class: DriftClass, dispatched: bool) -> String {
    let finding = report.changes.iter().find(|c| c.class == class);
    let field = finding
        .map(|c| display_path(&c.path))
        .unwrap_or_else(|| "$".into());
    let instruction = match (class, finding) {
        (DriftClass::Shape, Some(c))
            if c.kind == Kind::Renamed
                && c.reason == Reason::ConfirmedIdentity
                && c.path.chars().count() <= 256
                && c.to_path
                    .as_ref()
                    .is_some_and(|target| !target.is_empty() && target.chars().count() <= 256) =>
        {
            let target = c.to_path.as_deref().map(display_path).unwrap_or_default();
            format!(
                "Confirmed field rename `{field}` → `{target}`. Update the call to use `{target}` and validate it against the authoritative schema."
            )
        }
        (DriftClass::Shape, Some(c)) if c.kind == Kind::MalformedBody => {
            format!(
                "Body at `{field}` is malformed. Supply a complete valid JSON body matching the authoritative schema; missing bytes cannot be reconstructed."
            )
        }
        (DriftClass::Shape, _) => format!(
            "Check field `{field}` against the authoritative schema for its expected name, type and requiredness; that expected form is unavailable here. Correct and validate the call before retrying."
        ),
        (DriftClass::Semantic, _) => format!(
            "Verify the meaning, units and allowed values of field `{field}` against the authoritative contract. Obtain human review before changing or resubmitting the call; no automatic conversion is authorized."
        ),
        (DriftClass::Unknown, _) => format!(
            "Obtain authoritative schema and meaning evidence for field `{field}` and human review before changing or resubmitting the call; the expected form is unknown."
        ),
    };
    let disposition = if dispatched {
        "The upstream call was already dispatched and may have completed. Do not blindly replay; verify its outcome and idempotency first."
    } else {
        "The upstream call was not dispatched."
    };
    format!("{instruction} {disposition}")
}
// Paths are evidence, never scalar values. Bound diagnostic output and escape
// control characters/backticks so field names cannot break hint delimiters.
fn display_path(path: &str) -> String {
    let mut result: String = path
        .chars()
        .take(256)
        .flat_map(|c| {
            if c == '`' {
                "\\u0060".chars().collect::<Vec<_>>()
            } else {
                c.escape_debug().collect()
            }
        })
        .collect();
    if path.chars().nth(256).is_some() {
        result.push('…');
    }
    result
}
