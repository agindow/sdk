//! Checked text/version adapters preserve frozen finding v2 bounds in serde.
use crate::investigation_wire::Invalid;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Text<const CAP: usize>(String);
impl<const CAP: usize> TryFrom<String> for Text<CAP> {
    type Error = Invalid;
    fn try_from(value: String) -> Result<Self, Invalid> {
        if value.is_empty() || value.chars().count() > CAP {
            Err(Invalid)
        } else {
            Ok(Self(value))
        }
    }
}
impl<const CAP: usize> From<Text<CAP>> for String {
    fn from(value: Text<CAP>) -> Self {
        value.0
    }
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub struct Version;
impl TryFrom<u8> for Version {
    type Error = Invalid;
    fn try_from(value: u8) -> Result<Self, Invalid> {
        if value == 2 { Ok(Self) } else { Err(Invalid) }
    }
}
impl From<Version> for u8 {
    fn from(_: Version) -> Self {
        2
    }
}
