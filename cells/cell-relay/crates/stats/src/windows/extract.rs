use super::{Error, Limits};
use agw_inspector::{Inspection, Parse, Value};
use std::collections::BTreeSet;

pub(super) fn complete(inspection: &Inspection, limits: Limits) -> Result<&Value, Error> {
    complete_budget(inspection, limits).map(|(root, _)| root)
}
pub(super) fn complete_budget(
    inspection: &Inspection,
    mut limits: Limits,
) -> Result<(&Value, usize), Error> {
    if inspection.truncated {
        return Err(Error::Incomplete);
    }
    let Parse::Parsed(root) = &inspection.parse else {
        return Err(Error::Incomplete);
    };
    validate(
        root,
        &mut limits.inspection_nodes,
        &mut limits.inspection_bytes,
        0,
    )?;
    Ok((root, limits.inspection_nodes))
}
fn charge(text: &str, bytes: &mut usize) -> Result<(), Error> {
    *bytes = bytes.checked_sub(text.len()).ok_or(Error::ResourceLimit)?;
    Ok(())
}
fn validate(
    value: &Value,
    nodes: &mut usize,
    bytes: &mut usize,
    depth: usize,
) -> Result<(), Error> {
    if depth >= 128 {
        return Err(Error::ResourceLimit);
    }
    *nodes = nodes.checked_sub(1).ok_or(Error::ResourceLimit)?;
    match value {
        Value::Object(entries) => {
            let mut keys = BTreeSet::new();
            for (key, value) in entries {
                charge(key, bytes)?;
                if !keys.insert(key) {
                    return Err(Error::DuplicateKey);
                }
                validate(value, nodes, bytes, depth + 1)?;
            }
        }
        Value::Array(values) => {
            for value in values {
                validate(value, nodes, bytes, depth + 1)?;
            }
        }
        Value::String(text) => charge(text, bytes)?,
        Value::Number(n) => {
            *bytes = bytes
                .checked_sub(n.lexeme_len())
                .ok_or(Error::ResourceLimit)?;
            // The integer variants are canonical by construction. Retained
            // lexemes are public and synthetic values must still be rejected.
            if matches!(
                n,
                agw_inspector::Number::Verbatim(_) | agw_inspector::Number::Unrepresentable(_)
            ) {
                let token = n.lexeme();
                let raw: &serde_json::value::RawValue =
                    serde_json::from_str(&token).map_err(|_| Error::InvalidInput)?;
                if raw.get() != token
                    || !matches!(token.as_bytes().first(), Some(b'-' | b'0'..=b'9'))
                {
                    return Err(Error::InvalidInput);
                }
            }
        }
        _ => {}
    }
    Ok(())
}

pub(super) fn scalar<'a>(root: &'a Value, path: &str) -> Result<Option<&'a Value>, Error> {
    let mut work = usize::MAX;
    scalar_work(root, path, &mut work)
}
pub(super) fn scalar_work<'a>(
    root: &'a Value,
    path: &str,
    work: &mut usize,
) -> Result<Option<&'a Value>, Error> {
    let current = node_work(root, path, work)?;
    if matches!(current, Some(Value::Array(_) | Value::Object(_))) {
        return Err(Error::UnsupportedPath);
    }
    Ok(current)
}

pub(super) fn node<'a>(root: &'a Value, path: &str) -> Result<Option<&'a Value>, Error> {
    let mut work = usize::MAX;
    node_work(root, path, &mut work)
}
pub(super) fn node_work<'a>(
    root: &'a Value,
    path: &str,
    work: &mut usize,
) -> Result<Option<&'a Value>, Error> {
    if !path.starts_with('/') || path.len() > 4096 {
        return Err(Error::UnsupportedPath);
    }
    let mut current = Some(root);
    for part in path[1..].split('/') {
        *work = work.checked_sub(1).ok_or(Error::ResourceLimit)?;
        // Reject invalid escapes even if an earlier property is absent.
        let mut key = String::new();
        let mut chars = part.chars();
        while let Some(c) = chars.next() {
            key.push(match c {
                '~' => match chars.next() {
                    Some('0') => '~',
                    Some('1') => '/',
                    _ => return Err(Error::UnsupportedPath),
                },
                _ => c,
            });
        }
        current = match current {
            None => None,
            Some(Value::Object(entries)) => {
                let mut found = None;
                for (k, v) in entries {
                    *work = work.checked_sub(1).ok_or(Error::ResourceLimit)?;
                    if k == &key {
                        found = Some(v);
                        break;
                    }
                }
                found
            }
            _ => return Err(Error::UnsupportedPath),
        };
    }
    Ok(current)
}
