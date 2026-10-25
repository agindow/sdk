//! TI-38: bounded, precompiled per-field precision hints. No magnitude inference.
use crate::Precision;
use std::{collections::BTreeMap, error::Error, fmt};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrecisionError {
    EmptyPath,
    DuplicatePath,
    ResourceLimit,
}
impl fmt::Display for PrecisionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "precision map: {self:?}")
    }
}
impl Error for PrecisionError {}

/// Compile outside forwarding. Paths match numeric leaves exactly, with `[]`
/// pooling array indices. Container flags do not propagate to descendants.
#[derive(Debug, Clone, Default)]
pub struct FieldPrecisions(BTreeMap<String, Precision>);
impl FieldPrecisions {
    pub fn new<'a>(
        fields: impl IntoIterator<Item = (&'a str, Option<bool>)>,
    ) -> Result<Self, PrecisionError> {
        let mut result = BTreeMap::new();
        let mut text = 0usize;
        for (path, flag) in fields {
            if path.is_empty() {
                return Err(PrecisionError::EmptyPath);
            }
            text = text.saturating_add(path.len());
            if result.len() >= 4096 || text > 1024 * 1024 {
                return Err(PrecisionError::ResourceLimit);
            }
            let precision = if flag == Some(true) {
                Precision::HighPrecision
            } else {
                Precision::Observed
            };
            if result.insert(path.into(), precision).is_some() {
                return Err(PrecisionError::DuplicatePath);
            }
        }
        Ok(Self(result))
    }
    pub fn at(&self, path: &str) -> Precision {
        self.0.get(path).copied().unwrap_or_default()
    }
}

#[derive(Clone, Copy)]
pub(crate) enum Policy<'a> {
    Whole(Precision),
    Fields(&'a FieldPrecisions),
}
impl Policy<'_> {
    pub fn at(self, path: &str) -> Precision {
        match self {
            Self::Whole(p) => p,
            Self::Fields(fields) => fields.at(if path.is_empty() { "$" } else { path }),
        }
    }
    pub fn push_child(self, path: &mut String, key: &str) {
        if matches!(self, Self::Fields(_)) {
            if !path.is_empty() {
                path.push('.');
            }
            append_key(path, key);
        }
    }
    pub fn push_items(self, path: &mut String) {
        if matches!(self, Self::Fields(_)) {
            if path.is_empty() {
                path.push('$');
            }
            path.push_str("[]");
        }
    }
}

fn append_key(path: &mut String, key: &str) {
    if key.is_empty() {
        path.push_str("\\e");
    }
    for c in key.chars() {
        if matches!(c, '\\' | '.' | '[' | ']' | '$') {
            path.push('\\');
        }
        path.push(c);
    }
}
pub(crate) fn child(parent: &str, key: &str) -> String {
    let mut path = if parent == "$" {
        String::new()
    } else {
        format!("{parent}.")
    };
    append_key(&mut path, key);
    path
}
