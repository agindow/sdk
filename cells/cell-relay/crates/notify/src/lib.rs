//! TII-28 channel-neutral notification envelopes and bounded emission.
//!
//! This crate never runs on a guarded call path. [`envelope`] builds the closed
//! `notification` v1 message from an eligible TI-19 alert; [`emitter`] is a
//! volatile, bounded handoff a host drains from its own scheduler. Nothing here
//! performs network I/O, throws into a call, or claims durable admission.
//!
//! Inherited constraints: propagate with `?`; identifiers are 1–256 bytes with
//! no control characters; references only — never customer payload bytes,
//! replacement tokens or credentials (contract §10.39).

pub mod emitter;
pub mod envelope;

/// A rejected envelope or an exhausted bound. Never a guard error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// A supplied value violates a documented rule; the reason is stable.
    Invalid(&'static str),
    /// The emitter already holds this notification with different content.
    Conflict,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(reason) => write!(f, "invalid notification: {reason}"),
            Self::Conflict => write!(f, "conflicting notification"),
        }
    }
}

impl std::error::Error for Error {}

/// One bounded identifier: 1–256 UTF-8 bytes, no control characters.
pub(crate) fn identifier(value: &str) -> Result<(), Error> {
    if value.is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
        return Err(Error::Invalid("invalid notification identifier"));
    }
    Ok(())
}

/// Credential-shaped values and payload-byte shapes never travel as references.
///
/// Deliberately heuristic: the schema closes unknown fields, and this closes the
/// known secrets and encoded blobs that could otherwise hide inside a reference.
pub(crate) fn suspicious(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    if lower.starts_with("bearer ") || lower.starts_with("basic ") {
        return true;
    }
    if lower.contains("-----begin ") && lower.contains("private key-----") {
        return true;
    }
    const PREFIXES: [&str; 10] = [
        "sk_live_", "sk_test_", "akia", "ghp_", "gho_", "ghu_", "ghs_", "ghr_", "xoxb-", "xoxp-",
    ];
    if PREFIXES.iter().any(|prefix| lower.starts_with(prefix)) {
        return true;
    }
    if lower.starts_with("eyj")
        && value.matches('.').count() == 2
        && !value.contains(char::is_whitespace)
    {
        return true;
    }
    const ASSIGNMENTS: [&str; 10] = [
        "password=",
        "password:",
        "api_key=",
        "apikey=",
        "token=",
        "token:",
        "secret=",
        "secret:",
        "credential=",
        "authorization:",
    ];
    if ASSIGNMENTS.iter().any(|pattern| lower.contains(pattern)) {
        return true;
    }
    if value.len() >= 64
        && value.len().is_multiple_of(2)
        && value.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return true;
    }
    lower.starts_with('{')
        || lower.starts_with('[')
        || (lower.starts_with("data:") && lower.contains('/'))
}
