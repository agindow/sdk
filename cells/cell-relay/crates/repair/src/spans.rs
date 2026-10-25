//! Validate the tee against borrowed raw tokens and locate exact editable spans.
use crate::path::child;
use crate::raw::{Children, children};
use crate::{Failure, JsonType, Limits};
use agw_inspector::Value;
use serde_json::value::RawValue;
use std::{collections::BTreeSet, ops::Range};

pub(crate) struct Span {
    pub path: String,
    pub schema_path: String,
    pub token: Range<usize>,
    pub key_token: Option<Range<usize>>,
    pub key: Option<String>,
    pub parent: Option<usize>,
    pub kind: JsonType,
    pub keys: BTreeSet<String>,
}

pub(crate) fn index(source: &[u8], value: &Value, limits: Limits) -> Result<Vec<Span>, Failure> {
    let raw: &RawValue = serde_json::from_slice(source).map_err(|_| Failure::InspectionMismatch)?;
    let mut builder = Builder {
        source,
        limits,
        spans: Vec::new(),
        path_bytes: 0,
    };
    builder.visit(
        raw,
        value,
        Span {
            path: "$".into(),
            schema_path: "$".into(),
            token: 0..0,
            key_token: None,
            key: None,
            parent: None,
            kind: kind(value),
            keys: BTreeSet::new(),
        },
        0,
    )?;
    Ok(builder.spans)
}

pub(crate) fn raw_kind(raw: &RawValue) -> JsonType {
    match raw.get().as_bytes().first() {
        Some(b'{') => JsonType::Object,
        Some(b'[') => JsonType::Array,
        Some(b'"') => JsonType::String,
        Some(b'n') => JsonType::Null,
        Some(b't' | b'f') => JsonType::Bool,
        _ => JsonType::Number,
    }
}
fn kind(value: &Value) -> JsonType {
    match value {
        Value::Null => JsonType::Null,
        Value::Bool(_) => JsonType::Bool,
        Value::Number(_) => JsonType::Number,
        Value::String(_) => JsonType::String,
        Value::Array(_) => JsonType::Array,
        Value::Object(_) => JsonType::Object,
    }
}

struct Builder<'a> {
    source: &'a [u8],
    limits: Limits,
    spans: Vec<Span>,
    path_bytes: usize,
}
impl Builder<'_> {
    fn range(&self, raw: &RawValue) -> Result<Range<usize>, Failure> {
        let start = raw
            .get()
            .as_ptr()
            .addr()
            .checked_sub(self.source.as_ptr().addr())
            .ok_or(Failure::InspectionMismatch)?;
        let end = start
            .checked_add(raw.get().len())
            .ok_or(Failure::ResourceLimit)?;
        if end > self.source.len() {
            return Err(Failure::InspectionMismatch);
        }
        Ok(start..end)
    }
    fn visit(
        &mut self,
        raw: &RawValue,
        value: &Value,
        mut span: Span,
        depth: usize,
    ) -> Result<(), Failure> {
        if depth > self.limits.max_depth || self.spans.len() >= self.limits.max_nodes {
            return Err(Failure::ResourceLimit);
        }
        self.path_bytes = self
            .path_bytes
            .saturating_add(span.path.len())
            .saturating_add(span.schema_path.len());
        if self.path_bytes > self.limits.max_bytes {
            return Err(Failure::ResourceLimit);
        }
        if raw_kind(raw) != kind(value) {
            return Err(Failure::InspectionMismatch);
        }
        span.token = self.range(raw)?;
        let idx = self.spans.len();
        self.spans.push(span);
        match value {
            Value::Object(values) => {
                let Children::Object(entries) =
                    children(raw, self.limits.max_nodes - self.spans.len())?
                else {
                    return Err(Failure::InspectionMismatch);
                };
                if entries.len() != values.len() {
                    return Err(Failure::InspectionMismatch);
                }
                let mut cursor = self.spans[idx].token.start + 1;
                for ((key, raw), (expected, value)) in entries.into_iter().zip(values) {
                    if !self.spans[idx].keys.insert(key.clone()) {
                        return Err(Failure::DuplicateKeys);
                    }
                    if &key != expected {
                        return Err(Failure::InspectionMismatch);
                    }
                    while self
                        .source
                        .get(cursor)
                        .is_some_and(|b| b.is_ascii_whitespace() || *b == b',')
                    {
                        cursor += 1;
                    }
                    let end = string_end(self.source, cursor).ok_or(Failure::InspectionMismatch)?;
                    let decoded: String = serde_json::from_slice(&self.source[cursor..end])
                        .map_err(|_| Failure::InspectionMismatch)?;
                    if decoded != key {
                        return Err(Failure::InspectionMismatch);
                    }
                    self.visit(
                        raw,
                        value,
                        Span {
                            path: child(&self.spans[idx].path, &key),
                            schema_path: child(&self.spans[idx].schema_path, &key),
                            token: 0..0,
                            key_token: Some(cursor..end),
                            key: Some(key),
                            parent: Some(idx),
                            kind: kind(value),
                            keys: BTreeSet::new(),
                        },
                        depth + 1,
                    )?;
                    cursor = self.range(raw)?.end;
                }
            }
            Value::Array(values) => {
                let Children::Array(entries) =
                    children(raw, self.limits.max_nodes - self.spans.len())?
                else {
                    return Err(Failure::InspectionMismatch);
                };
                if entries.len() != values.len() {
                    return Err(Failure::InspectionMismatch);
                }
                for (i, (raw, value)) in entries.into_iter().zip(values).enumerate() {
                    self.visit(
                        raw,
                        value,
                        Span {
                            path: format!("{}[{i}]", self.spans[idx].path),
                            schema_path: format!("{}[]", self.spans[idx].schema_path),
                            token: 0..0,
                            key_token: None,
                            key: None,
                            parent: Some(idx),
                            kind: kind(value),
                            keys: BTreeSet::new(),
                        },
                        depth + 1,
                    )?;
                }
            }
            Value::String(expected) => {
                let actual: String =
                    serde_json::from_str(raw.get()).map_err(|_| Failure::InspectionMismatch)?;
                if &actual != expected {
                    return Err(Failure::InspectionMismatch);
                }
            }
            Value::Number(expected) if expected.lexeme().as_ref() != raw.get() => {
                return Err(Failure::InspectionMismatch);
            }
            Value::Bool(expected)
                if (*expected && raw.get() != "true") || (!expected && raw.get() != "false") =>
            {
                return Err(Failure::InspectionMismatch);
            }
            _ => {}
        }
        Ok(())
    }
}

fn string_end(source: &[u8], start: usize) -> Option<usize> {
    if source.get(start) != Some(&b'"') {
        return None;
    }
    let mut cursor = start + 1;
    while let Some(byte) = source.get(cursor) {
        match byte {
            b'"' => return Some(cursor + 1),
            b'\\' => cursor += 2,
            _ => cursor += 1,
        }
    }
    None
}
