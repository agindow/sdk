use super::{CLOCK_SKEW_SECONDS, Denial};
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AssertionHeader {
    pub alg: String,
    pub typ: Option<String>,
    pub kid: Option<String>,
}
#[derive(Deserialize)]
#[serde(untagged)]
enum Audience {
    One(String),
    Many(Vec<String>),
}
#[derive(Deserialize)]
pub(crate) struct Subject {
    iss: Option<String>,
    sub: String,
    aud: Audience,
    iat: Option<u64>,
    nbf: Option<u64>,
    exp: u64,
    pub jti: Option<String>,
}
impl Subject {
    pub fn check(&self, r: &super::Registration, now: u64) -> Result<(), Denial> {
        let spiffe = r.subject.starts_with("spiffe://");
        let audience = match &self.aud {
            Audience::One(a) => a == &r.audience,
            Audience::Many(a) => {
                spiffe && !a.is_empty() && a.len() <= 8 && a.iter().any(|a| a == &r.audience)
            }
        };
        if self.sub != r.subject
            || !audience
            || self.iss.as_deref().is_some_and(|i| i != r.issuer)
            || (!spiffe && self.iss.is_none())
        {
            return Err(Denial::Registration);
        }
        if self.exp.saturating_add(CLOCK_SKEW_SECONDS) <= now
            || self
                .nbf
                .is_some_and(|n| n > now.saturating_add(CLOCK_SKEW_SECONDS))
            || self
                .iat
                .is_some_and(|i| i > now.saturating_add(CLOCK_SKEW_SECONDS) || self.exp <= i)
            || (!spiffe && self.iat.is_none())
            || (r.private_key_jwt && self.iat.is_none_or(|i| self.exp.saturating_sub(i) > 300))
        {
            return Err(Denial::Expiry);
        }
        Ok(())
    }
    pub fn replay_until(&self) -> u64 {
        self.exp.saturating_add(CLOCK_SKEW_SECONDS)
    }
}
