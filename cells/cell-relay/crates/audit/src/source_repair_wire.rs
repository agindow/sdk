//! Structural source-repair adapters; semantic scope/time/digest checks stay off path in TS.
use crate::investigation_wire::Invalid;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Field(String);
impl TryFrom<String> for Field {
    type Error = Invalid;
    fn try_from(value: String) -> Result<Self, Invalid> {
        let field = value.strip_prefix('/').ok_or(Invalid)?;
        let mut bytes = field.bytes();
        let first = bytes.next().ok_or(Invalid)?;
        if value.len() > 129
            || !(first.is_ascii_alphabetic() || first == b'_')
            || !bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        {
            return Err(Invalid);
        }
        Ok(Self(value))
    }
}
impl From<Field> for String {
    fn from(value: Field) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Digest(String);
impl TryFrom<String> for Digest {
    type Error = Invalid;
    fn try_from(value: String) -> Result<Self, Invalid> {
        if value.len() != 64
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(Invalid);
        }
        Ok(Self(value))
    }
}
impl From<Digest> for String {
    fn from(value: Digest) -> Self {
        value.0
    }
}
