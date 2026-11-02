//! Explicit concrete parent selections. No wildcard expansion or container creation.
use agw_inspector::Value;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Selection {
    source: String,
    parents: Vec<String>,
}

pub fn prepare(
    bytes: &[u8],
    selection: &str,
    target: &str,
    confidence: f64,
    mode: &str,
    work: u32,
) -> Result<String, String> {
    if !matches!(mode, "observe" | "repair" | "strict") {
        return Err("invalid mode".into());
    }
    if selection.len() > 16_384 {
        return Err("rename selection limit".into());
    }
    let selection: Selection =
        serde_json::from_str(selection).map_err(|_| "invalid rename selection")?;
    if selection.parents.is_empty()
        || selection.parents.len() > 16
        || !key(&selection.source)
        || !key(target)
        || selection.source == target
    {
        return Err("invalid reviewed rename".into());
    }
    let mut pointers = Vec::new();
    for parent in &selection.parents {
        let mut pointer = parse(parent)?;
        pointer.push(selection.source.clone());
        pointers.push(pointer);
    }
    for (i, pointer) in pointers.iter().enumerate() {
        if pointers
            .iter()
            .skip(i + 1)
            .any(|other| pointer.starts_with(other) || other.starts_with(pointer))
        {
            return Err("overlapping rename selections".into());
        }
    }
    super::selective::bounded(bytes, confidence, work)?;
    let inspection = agw_inspector::inspect(bytes, false, None);
    let agw_inspector::Parse::Parsed(value) = &inspection.parse else {
        return Err("repair inspection unavailable".into());
    };
    validate(value, 0, &mut 0)?;
    let paths = pointers
        .iter()
        .map(|pointer| resolve(value, pointer))
        .collect::<Option<Vec<_>>>();
    let Some(paths) = paths else {
        // An explicit set is all-or-none, including absent parents or members.
        return Ok(
            serde_json::json!({"matched":true,"applied":false,"halt":mode == "strict"}).to_string(),
        );
    };
    super::selective::prepare_paths(bytes, &inspection, paths, target, confidence, mode)
}

fn key(value: &str) -> bool {
    value.len() <= 64 && !value.chars().any(|c| c <= '\u{1f}' || c == '\u{7f}')
}
pub(crate) fn parse(pointer: &str) -> Result<Vec<String>, String> {
    if pointer.len() > 512 || (!pointer.is_empty() && !pointer.starts_with('/')) {
        return Err("invalid parent pointer".into());
    }
    if pointer.is_empty() {
        return Ok(Vec::new());
    }
    let mut parts = Vec::new();
    for part in pointer[1..].split('/') {
        let mut decoded = String::new();
        let mut chars = part.chars();
        while let Some(c) = chars.next() {
            decoded.push(if c == '~' {
                match chars.next() {
                    Some('0') => '~',
                    Some('1') => '/',
                    _ => return Err("invalid pointer escape".into()),
                }
            } else {
                c
            });
        }
        parts.push(decoded);
    }
    if parts.len() > 32 {
        return Err("parent pointer depth limit".into());
    }
    Ok(parts)
}
pub(crate) fn escape(key: &str) -> String {
    if key.is_empty() {
        return "\\e".into();
    }
    key.chars()
        .flat_map(|c| {
            if matches!(c, '\\' | '.' | '[' | ']' | '$') {
                vec!['\\', c]
            } else {
                vec![c]
            }
        })
        .collect()
}
pub(crate) fn resolve(mut value: &Value, pointer: &[String]) -> Option<String> {
    let mut path = String::new();
    for part in pointer {
        match value {
            Value::Object(entries) => {
                let mut matching = entries.iter().filter(|(key, _)| key == part);
                value = &matching.next()?.1;
                if matching.next().is_some() {
                    return None;
                }
                if !path.is_empty() {
                    path.push('.');
                }
                path.push_str(&escape(part));
            }
            Value::Array(entries) => {
                let index: usize = part.parse().ok()?;
                if index.to_string() != *part {
                    return None;
                }
                value = entries.get(index)?;
                if path.is_empty() {
                    path.push('$');
                }
                path.push_str(&format!("[{index}]"));
            }
            _ => return None,
        }
    }
    Some(path)
}

pub(crate) fn validate(value: &Value, depth: usize, nodes: &mut usize) -> Result<(), String> {
    *nodes += 1;
    if *nodes > 4096 || depth > 64 {
        return Err("repair inspection limit".into());
    }
    match value {
        Value::Object(entries) => {
            let mut keys = std::collections::BTreeSet::new();
            for (key, value) in entries {
                if !keys.insert(key) {
                    return Err("duplicate repair member".into());
                }
                validate(value, depth + 1, nodes)?;
            }
        }
        Value::Array(entries) => {
            for value in entries {
                validate(value, depth + 1, nodes)?;
            }
        }
        _ => {}
    }
    Ok(())
}
