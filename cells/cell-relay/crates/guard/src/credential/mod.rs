//! EPIC-13: bounded post-response evidence. Never reads request credentials.
pub mod cutover;
pub mod health;
pub mod tracker;
use crate::proxy::Headers;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cause {
    Rejected,
    Expired,
    Revoked,
    ScopeReduced,
    Quota,
    Rotated,
    Recovered,
    Unknown,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Evidence {
    ProviderDeclared,
    HostDeclared,
    Inferred,
    Unknown,
}
impl Cause {
    pub fn failure(self) -> bool {
        matches!(
            self,
            Self::Rejected | Self::Expired | Self::Revoked | Self::ScopeReduced
        )
    }
    pub fn terminal(self, evidence: Evidence) -> bool {
        matches!(self, Self::Expired | Self::Revoked)
            && matches!(
                evidence,
                Evidence::ProviderDeclared | Evidence::HostDeclared
            )
    }
}
/// Only bounded response input is inspected. The host attests completeness and
/// availability; cap-truncated bodies and bypasses must never imply vendor drift.
pub struct Signal<'a> {
    pub status: u16,
    pub headers: &'a Headers,
    pub body: Option<&'a [u8]>,
    pub complete: bool,
    pub available: bool,
    pub revoked: bool,
}
pub fn classify(signal: &Signal<'_>) -> Option<(Cause, Evidence)> {
    if !matches!(signal.status, 401 | 403) || !complete_evidence(signal) {
        return None;
    }
    classify_complete(signal)
}

fn complete_evidence(signal: &Signal<'_>) -> bool {
    if !signal.available || !signal.complete || signal.headers.iter().take(129).count() > 128 {
        return false;
    }
    if signal.headers.iter().take(128).any(|(name, value)| {
        (name.eq_ignore_ascii_case("www-authenticate") && value.len() > 4096)
            || ((name.eq_ignore_ascii_case("retry-after")
                || name.eq_ignore_ascii_case("x-ratelimit-remaining"))
                && value.len() > 128)
    }) {
        return false;
    }
    if let Some(body) = signal.body
        && (body.len() > 4096 || serde_json::from_slice::<serde::de::IgnoredAny>(body).is_err())
    {
        return false;
    }
    true
}

fn classify_complete(signal: &Signal<'_>) -> Option<(Cause, Evidence)> {
    if signal.status == 401 {
        if signal.revoked {
            return Some((Cause::Revoked, Evidence::HostDeclared));
        }
        let mut rejected = false;
        for (_, value) in signal
            .headers
            .iter()
            .take(128)
            .filter(|(name, _)| name.eq_ignore_ascii_case("www-authenticate"))
            .take(16)
        {
            if value.len() > 4096 {
                continue;
            }
            match auth_error(value) {
                Some(Cause::Expired) => return Some((Cause::Expired, Evidence::ProviderDeclared)),
                Some(Cause::Rejected) => rejected = true,
                _ => {}
            }
        }
        return Some((
            Cause::Rejected,
            if rejected {
                Evidence::ProviderDeclared
            } else {
                Evidence::Unknown
            },
        ));
    }
    if signal.status == 403 {
        let quota = signal.headers.iter().take(128).any(|(name, value)| {
            (name.eq_ignore_ascii_case("retry-after") && !value.is_empty() && value.len() <= 128)
                || (name.eq_ignore_ascii_case("x-ratelimit-remaining") && value.trim() == "0")
        }) || signal
            .body
            .filter(|body| body.len() <= 4096)
            .is_some_and(|body| {
                [
                    b"quota exceeded".as_slice(),
                    b"rate limit exceeded",
                    b"plan limit exceeded",
                ]
                .iter()
                .any(|phrase| {
                    body.windows(phrase.len())
                        .any(|window| window.eq_ignore_ascii_case(phrase))
                })
            });
        if quota {
            return Some((Cause::Quota, Evidence::ProviderDeclared));
        }
    }
    None
}

pub mod session;

// Scan complete auth parameters rather than searching substrings: quoted realm
// or error_description content must never acquire provider authority.
fn auth_error(header: &str) -> Option<Cause> {
    let bytes = header.as_bytes();
    let mut at = 0;
    let mut found = None;
    while at < bytes.len() {
        while bytes
            .get(at)
            .is_some_and(|b| b.is_ascii_whitespace() || *b == b',')
        {
            at += 1;
        }
        if at == bytes.len() {
            break;
        }
        let start = at;
        while bytes
            .get(at)
            .is_some_and(|b| b.is_ascii_alphanumeric() || matches!(*b, b'_' | b'-'))
        {
            at += 1;
        }
        if start == at {
            return None;
        }
        let key = header.get(start..at)?;
        while bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
            at += 1;
        }
        if bytes.get(at) != Some(&b'=') {
            continue;
        }
        at += 1;
        while bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
            at += 1;
        }
        let quoted = bytes.get(at) == Some(&b'"');
        if quoted {
            at += 1;
        }
        let start = at;
        let mut escaped = false;
        if quoted {
            while let Some(byte) = bytes.get(at) {
                if *byte == b'"' {
                    break;
                }
                if *byte == b'\\' {
                    escaped = true;
                    at += 1;
                    bytes.get(at)?;
                }
                at += 1;
            }
            if bytes.get(at) != Some(&b'"') {
                return None;
            }
        } else {
            while bytes
                .get(at)
                .is_some_and(|b| !b.is_ascii_whitespace() && *b != b',')
            {
                at += 1;
            }
        }
        let value = header.get(start..at)?;
        if quoted {
            at += 1;
        }
        if key.eq_ignore_ascii_case("error") && !escaped {
            match value {
                "expired_token" => found = Some(Cause::Expired),
                "invalid_token" if found.is_none() => found = Some(Cause::Rejected),
                _ => {}
            }
        }
    }
    found
}

/// Explicit bounded host extension; phrases are response markers, never tokens.
pub fn classify_with_phrases(signal: &Signal<'_>, phrases: &[&str]) -> Option<(Cause, Evidence)> {
    if signal.status != 403 {
        return classify(signal);
    }
    if !complete_evidence(signal) {
        return None;
    }
    let base = classify_complete(signal);
    if base.is_some() || phrases.len() > 16 {
        return base;
    }
    let body = signal.body.filter(|body| body.len() <= 4096)?;
    if serde_json::from_slice::<serde::de::IgnoredAny>(body).is_err() {
        return None;
    }
    if phrases.iter().any(|phrase| {
        !phrase.is_empty()
            && phrase.len() <= 64
            && body
                .windows(phrase.len())
                .any(|part| part.eq_ignore_ascii_case(phrase.as_bytes()))
    }) {
        Some((Cause::Quota, Evidence::ProviderDeclared))
    } else {
        None
    }
}
