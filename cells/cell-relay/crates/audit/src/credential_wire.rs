//! TI-65 conditional validation beyond typify's structural deserializer.
use crate::contracts::credential_event::CredentialEvent;
use crate::investigation_wire::Invalid;
use serde_json::Value;
pub fn parse(bytes: &[u8]) -> Result<CredentialEvent, Invalid> {
    if bytes.len() > 8192 {
        return Err(Invalid);
    }
    let v: Value = serde_json::from_slice(bytes).map_err(|_| Invalid)?;
    let cause = v["cause"].as_str().ok_or(Invalid)?;
    let evidence = v["evidence"].as_str().ok_or(Invalid)?;
    let ts = v["ts"].as_str().ok_or(Invalid)?;
    if !ts.ends_with('Z') || chrono::DateTime::parse_from_rfc3339(ts).is_err() {
        return Err(Invalid);
    }
    let terminal = matches!(cause, "expired" | "revoked");
    if terminal && !matches!(evidence, "provider_declared" | "host_declared")
        || (cause == "scope_reduced")
            != (v.get("failure_count").is_some() && v.get("window_ms").is_some())
        || (cause != "scope_reduced"
            && (v.get("failure_count").is_some() || v.get("window_ms").is_some()))
        || (cause == "scope_reduced" && evidence != "inferred")
        || (cause == "rotated") != v.get("prior_version").is_some()
        || (cause == "rotated"
            && (evidence != "host_declared" || v.get("credential_version").is_none()))
        || (cause == "recovered") != v.get("prior_event").is_some()
        || (cause == "recovered" && evidence == "unknown")
    {
        return Err(Invalid);
    }
    for key in [
        "id",
        "tenant",
        "connection",
        "operation",
        "credential_version",
        "prior_version",
        "prior_event",
        "audit_ref",
    ] {
        if let Some(s) = v.get(key).and_then(Value::as_str)
            && (s.is_empty()
                || s.len() > 256
                || s.chars().any(char::is_control)
                || ["sk_live_", "Bearer ", "Authorization", "secret="]
                    .iter()
                    .any(|p| s.contains(p)))
        {
            return Err(Invalid);
        }
    }
    for (field, min, max) in [
        ("status", 100, 599),
        ("failure_count", 1, 1000000),
        ("window_ms", 1, 86400000),
    ] {
        if let Some(n) = v.get(field)
            && n.as_u64().is_none_or(|n| n < min || n > max)
        {
            return Err(Invalid);
        }
    }
    serde_json::from_value(v).map_err(|_| Invalid)
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct UtcTimestamp(String);
impl TryFrom<String> for UtcTimestamp {
    type Error = Invalid;
    fn try_from(s: String) -> Result<Self, Invalid> {
        if !(20..=30).contains(&s.len())
            || !s.ends_with('Z')
            || chrono::DateTime::parse_from_rfc3339(&s).is_err()
        {
            return Err(Invalid);
        }
        Ok(Self(s))
    }
}
impl From<UtcTimestamp> for String {
    fn from(s: UtcTimestamp) -> Self {
        s.0
    }
}
