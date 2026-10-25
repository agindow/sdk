//! Checked dashboard query identifiers avoid regex initialization in generated contracts.
use crate::investigation_wire::Invalid;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Search(String);
impl TryFrom<String> for Search {
    type Error = Invalid;
    fn try_from(value: String) -> Result<Self, Invalid> {
        if value.is_empty()
            || value.len() > 128
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"-_:.".contains(&byte))
        {
            Err(Invalid)
        } else {
            Ok(Self(value))
        }
    }
}
impl From<Search> for String {
    fn from(value: Search) -> Self {
        value.0
    }
}
