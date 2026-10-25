//! Scoped hosted credentials and bounded, explicitly isolate-local quota windows.
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use subtle::ConstantTimeEq;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Credential {
    pub digest: String,
    pub tenant: String,
    pub routes: Vec<String>,
    pub plan: String,
    pub limit: u32,
    pub window_seconds: u32,
}
pub struct Auth {
    credentials: Vec<(Credential, [u8; 32])>,
    quotas: BTreeMap<usize, (u64, u32)>,
}
pub struct Permit {
    pub tenant: String,
    pub remaining: u32,
    pub limit: u32,
    pub resets: u64,
}
#[derive(Debug, PartialEq, Eq)]
pub enum Refusal {
    Unauthorized,
    Scope,
    Limited,
}
impl Auth {
    pub fn new(json: &str) -> Result<Self, &'static str> {
        if json.len() > 262144 {
            return Err("key manifest too large");
        }
        let supplied: Vec<Credential> =
            serde_json::from_str(json).map_err(|_| "invalid scoped keys")?;
        if supplied.is_empty() || supplied.len() > 256 {
            return Err("configure 1–256 keys");
        }
        let mut credentials = Vec::new();
        for credential in supplied {
            if credential.digest.len() != 64
                || !credential.digest.bytes().all(|c| c.is_ascii_hexdigit())
                || credential.tenant.is_empty()
                || credential.tenant.len() > 256
                || credential.routes.is_empty()
                || credential.routes.len() > 256
                || credential.plan.is_empty()
                || credential.limit == 0
                || !(1..=86400).contains(&credential.window_seconds)
            {
                return Err("invalid scoped key");
            }
            let mut digest = [0; 32];
            for (offset, byte) in digest.iter_mut().enumerate() {
                *byte = u8::from_str_radix(&credential.digest[offset * 2..offset * 2 + 2], 16)
                    .map_err(|_| "invalid digest")?;
            }
            if credentials
                .iter()
                .any(|(_, prior): &(Credential, [u8; 32])| prior == &digest)
            {
                return Err("duplicate key digest");
            }
            credentials.push((credential, digest));
        }
        Ok(Self {
            credentials,
            quotas: BTreeMap::new(),
        })
    }
    pub fn admit(
        &mut self,
        authorization: &str,
        route: &str,
        now_seconds: u64,
    ) -> Result<Permit, Refusal> {
        let token = authorization
            .strip_prefix("Bearer ")
            .ok_or(Refusal::Unauthorized)?;
        if token.is_empty() || token.len() > 1024 {
            return Err(Refusal::Unauthorized);
        }
        let digest: [u8; 32] = Sha256::digest(token.as_bytes()).into();
        // Visit every configured digest, avoiding a token-dependent early exit.
        let mut selected = None;
        for (index, (_, expected)) in self.credentials.iter().enumerate() {
            if bool::from(digest.ct_eq(expected)) {
                selected = Some(index);
            }
        }
        let index = selected.ok_or(Refusal::Unauthorized)?;
        let (credential, _) = &self.credentials[index];
        if !credential.routes.iter().any(|scope| scope == route) {
            return Err(Refusal::Scope);
        }
        let slot = now_seconds / u64::from(credential.window_seconds);
        let count = self.quotas.entry(index).or_insert((slot, 0));
        // A backwards wall-clock jump never opens a fresh allowance.
        if slot > count.0 {
            *count = (slot, 0);
        }
        if count.1 >= credential.limit {
            return Err(Refusal::Limited);
        }
        count.1 += 1;
        Ok(Permit {
            tenant: credential.tenant.clone(),
            remaining: credential.limit - count.1,
            limit: credential.limit,
            resets: (count.0 + 1) * u64::from(credential.window_seconds),
        })
    }
}
