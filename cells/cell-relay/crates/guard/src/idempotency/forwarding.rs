use super::{DurabilityStatus, Rejection, Request};
use crate::proxy::Headers;

/// Prepared once by the trusted host, with its endpoint's supported bindings.
#[derive(Debug, Clone)]
pub struct Bindings {
    key: String,
    durability: Option<String>,
}
impl Bindings {
    pub fn new(key_header: &str, durability_header: Option<&str>) -> Result<Self, Rejection> {
        if !header_name(key_header)
            || durability_header
                .is_some_and(|name| !header_name(name) || name.eq_ignore_ascii_case(key_header))
        {
            return Err(Rejection::invalid(
                "invalid or conflicting upstream header bindings",
            ));
        }
        Ok(Self {
            key: key_header.into(),
            durability: durability_header.map(str::to_owned),
        })
    }
    pub(super) fn prepare<'a>(&'a self, request: Request<'a>) -> Forwarding<'a> {
        let durability_status = match (request.durability, &self.durability) {
            (None, _) => DurabilityStatus::NotRequested,
            (Some(_), None) => DurabilityStatus::Unsupported,
            (Some(_), Some(_)) => DurabilityStatus::Forwarded,
        };
        Forwarding {
            key: request.key,
            durability: request.durability.filter(|_| self.durability.is_some()),
            durability_status,
            key_header: &self.key,
            durability_header: self.durability.as_deref(),
        }
    }
}
fn header_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte))
}
/// Reuse this immutable view on every explicit transport attempt. Forwarding a
/// hint is not proof the upstream persisted anything; no provider is inferred.
pub struct Forwarding<'a> {
    pub key: Option<&'a str>,
    pub durability: Option<&'a str>,
    pub durability_status: DurabilityStatus,
    key_header: &'a str,
    durability_header: Option<&'a str>,
}
impl Forwarding<'_> {
    pub fn apply_headers(&self, headers: &mut Headers) {
        if let Some(key) = self.key {
            headers.claim(self.key_header, key);
        }
        if let (Some(name), Some(value)) = (self.durability_header, self.durability) {
            headers.claim(name, value);
        }
    }
}
