//! Offline candidate derivation. Marginal profiles do not prove relationships.
use crate::{Error, Form, MAX_BINDINGS, MAX_ENUM, MAX_TEXT, Rule, compile, valid_text};

/// Caller attests that profiles cover complete comparable observations. Numeric
/// endpoints are exact i64; vocabulary is the full set, never a truncated top-k.
#[derive(Debug, Clone)]
pub enum Profile<'a> {
    Numeric {
        field: &'a str,
        samples: u64,
        complete: bool,
        min: i64,
        max: i64,
    },
    Vocabulary {
        field: &'a str,
        samples: u64,
        complete: bool,
        values: &'a [&'a str],
    },
}
impl Profile<'_> {
    fn field(&self) -> &str {
        match self {
            Self::Numeric { field, .. } | Self::Vocabulary { field, .. } => field,
        }
    }
}

/// Candidates only: publication and customer acceptance are explicit. Empty or
/// incomplete evidence is skipped; malformed evidence rejects the whole batch.
pub fn derive(profiles: &[Profile<'_>]) -> Result<Vec<Rule>, Error> {
    if profiles.len() > MAX_BINDINGS {
        return Err(Error::Capacity);
    }
    let mut candidates = Vec::new();
    for (i, profile) in profiles.iter().enumerate() {
        let field = profile.field();
        if !valid_text(field) || profiles[..i].iter().any(|p| p.field() == field) {
            return Err(Error::Invalid);
        }
        let form = match profile {
            Profile::Numeric {
                samples,
                complete,
                min,
                max,
                ..
            } => {
                if !complete || *samples == 0 {
                    continue;
                }
                if min > max {
                    return Err(Error::Invalid);
                }
                Form::Range {
                    field: field.into(),
                    min: *min,
                    max: *max,
                }
            }
            Profile::Vocabulary {
                samples,
                complete,
                values,
                ..
            } => {
                if !complete || *samples == 0 {
                    continue;
                }
                if values.is_empty() || values.len() > MAX_ENUM {
                    return Err(Error::Capacity);
                }
                for (j, value) in values.iter().enumerate() {
                    if value.len() > MAX_TEXT || values[..j].contains(value) {
                        return Err(Error::Invalid);
                    }
                }
                Form::Enum {
                    field: field.into(),
                    values: values.iter().map(|v| (*v).into()).collect(),
                }
            }
        };
        // Field names are unique, bounded stable candidate IDs within this scope.
        candidates.push(Rule {
            id: field.into(),
            form,
        });
    }
    let bindings: Vec<_> = profiles.iter().map(Profile::field).collect();
    compile(&candidates, &bindings)?;
    Ok(candidates)
}
