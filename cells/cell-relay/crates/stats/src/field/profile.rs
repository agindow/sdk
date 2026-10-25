use super::{Error, Fingerprint, Sentinel, Type};
use agw_inspector::{Number, Value};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Profile {
    opportunities: usize,
    types: [usize; 7],
    sentinels: [usize; 4],
    fingerprint: Option<Fingerprint>,
}
impl Profile {
    pub fn opportunities(&self) -> usize {
        self.opportunities
    }
    pub fn present(&self) -> usize {
        self.opportunities - self.sentinel_count(Sentinel::Missing)
    }
    pub fn non_null(&self) -> usize {
        self.present() - self.sentinel_count(Sentinel::Null)
    }
    pub fn completeness(&self) -> f64 {
        if self.opportunities == 0 {
            0.
        } else {
            self.non_null() as f64 / self.opportunities as f64
        }
    }
    pub fn type_count(&self, kind: Type) -> usize {
        self.types[kind as usize]
    }
    pub fn sentinel_count(&self, sentinel: Sentinel) -> usize {
        self.sentinels[sentinel as usize]
    }
    /// Absent when a comparison deliberately skipped unused fingerprint work.
    pub fn fingerprint(&self) -> Option<&Fingerprint> {
        self.fingerprint.as_ref()
    }
}

pub(super) fn text_budget(text: &str, used: &mut usize) -> Result<(), Error> {
    if text.len() > 4096 {
        return Err(Error::ResourceLimit);
    }
    *used += text.len();
    if *used > 1024 * 1024 {
        return Err(Error::ResourceLimit);
    }
    Ok(())
}

pub(super) fn build(
    samples: &[Option<&Value>],
    fingerprint: bool,
    used: &mut usize,
) -> Result<Profile, Error> {
    if samples.len() > 4096 {
        return Err(Error::ResourceLimit);
    }
    let mut result = Profile {
        opportunities: samples.len(),
        fingerprint: fingerprint.then(Fingerprint::default),
        ..Profile::default()
    };
    for sample in samples {
        let Some(value) = sample else {
            result.sentinels[Sentinel::Missing as usize] += 1;
            continue;
        };
        let kind = match value {
            Value::Null => {
                result.sentinels[Sentinel::Null as usize] += 1;
                Type::Null
            }
            Value::Bool(_) => Type::Boolean,
            Value::Array(_) => Type::Array,
            Value::Object(_) => Type::Object,
            Value::String(text) => {
                text_budget(text, used)?;
                result.sentinels[Sentinel::EmptyString as usize] += usize::from(text.is_empty());
                if let Some(fp) = &mut result.fingerprint {
                    fp.observe(text);
                }
                Type::String
            }
            Value::Number(n) => {
                // Integer variants are canonical by construction, so their
                // shape is known without formatting a String per sample. That
                // formatting dominated the detector loop for numeric windows.
                match n {
                    Number::I64(value) => {
                        *used += n.lexeme_len();
                        if *used > 1024 * 1024 {
                            return Err(Error::ResourceLimit);
                        }
                        result.sentinels[Sentinel::Zero as usize] += usize::from(*value == 0);
                        Type::Integer
                    }
                    Number::U64(value) => {
                        *used += n.lexeme_len();
                        if *used > 1024 * 1024 {
                            return Err(Error::ResourceLimit);
                        }
                        result.sentinels[Sentinel::Zero as usize] += usize::from(*value == 0);
                        Type::Integer
                    }
                    Number::Verbatim(_) | Number::Unrepresentable(_) => {
                        let token = n.lexeme();
                        text_budget(&token, used)?;
                        let raw: &serde_json::value::RawValue =
                            serde_json::from_str(&token).map_err(|_| Error::InvalidInput)?;
                        if raw.get() != token
                            || !matches!(token.as_bytes().first(), Some(b'-' | b'0'..=b'9'))
                        {
                            return Err(Error::InvalidInput);
                        }
                        let mantissa = token.split(['e', 'E']).next().ok_or(Error::InvalidInput)?;
                        let zero = mantissa.bytes().all(|b| matches!(b, b'-' | b'.' | b'0'));
                        result.sentinels[Sentinel::Zero as usize] += usize::from(zero);
                        if token.contains(['.', 'e', 'E']) {
                            Type::Number
                        } else {
                            Type::Integer
                        }
                    }
                }
            }
        };
        result.types[kind as usize] += 1;
    }
    Ok(result)
}

/// Full privately constructed profile. Empty windows are valid profiles, not
/// sufficient evidence; comparators require detector-specific sample floors.
pub fn profile(samples: &[Option<&Value>]) -> Result<Profile, Error> {
    build(samples, true, &mut 0)
}
