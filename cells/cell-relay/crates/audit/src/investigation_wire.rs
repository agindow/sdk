//! TII-25 checked native adapters for payload-free background jobs.
use crate::contracts::investigation_job::InvestigationJob;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy)]
pub struct Invalid;
impl std::fmt::Display for Invalid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("invalid investigation envelope")
    }
}
impl std::error::Error for Invalid {}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Amount(u64);
impl TryFrom<String> for Amount {
    type Error = Invalid;
    fn try_from(value: String) -> Result<Self, Invalid> {
        let amount = value.parse::<u64>().map_err(|_| Invalid)?;
        if amount.to_string() != value {
            return Err(Invalid);
        }
        Ok(Self(amount))
    }
}
impl From<Amount> for String {
    fn from(value: Amount) -> Self {
        value.0.to_string()
    }
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub struct Version;
impl TryFrom<u8> for Version {
    type Error = Invalid;
    fn try_from(value: u8) -> Result<Self, Invalid> {
        if value == 1 { Ok(Self) } else { Err(Invalid) }
    }
}
impl From<Version> for u8 {
    fn from(_: Version) -> Self {
        1
    }
}
fn text<'a>(value: &'a serde_json::Value, field: &str) -> Result<&'a str, Invalid> {
    value
        .get(field)
        .and_then(serde_json::Value::as_str)
        .ok_or(Invalid)
}
/// Both the generated frozen shape and the shared semantic bounds must pass.
/// This is background-only: it performs no source/provider/registry I/O.
pub fn validate(bytes: &[u8]) -> Result<InvestigationJob, Invalid> {
    if bytes.len() > 4096 {
        return Err(Invalid);
    }
    let wire: InvestigationJob = serde_json::from_slice(bytes).map_err(|_| Invalid)?;
    let value: serde_json::Value = serde_json::from_slice(bytes).map_err(|_| Invalid)?;
    for field in ["tenant", "id", "audit_id", "call_id", "price_context"] {
        let id = text(&value, field)?;
        if id.is_empty()
            || id.len() > 256
            || !id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_.:-".contains(&b))
        {
            return Err(Invalid);
        }
    }
    let created = text(&value, "created_at")?;
    let expires = text(&value, "expires_at")?;
    if !created.ends_with('Z') || !expires.ends_with('Z') {
        return Err(Invalid);
    }
    let created = chrono::DateTime::parse_from_rfc3339(created).map_err(|_| Invalid)?;
    let expires = chrono::DateTime::parse_from_rfc3339(expires).map_err(|_| Invalid)?;
    let lifetime = expires.signed_duration_since(created);
    if lifetime <= chrono::Duration::zero() || lifetime > chrono::Duration::minutes(10) {
        return Err(Invalid);
    }
    Ok(wire)
}
