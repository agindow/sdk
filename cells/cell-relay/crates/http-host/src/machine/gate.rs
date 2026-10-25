use super::{
    Claims, Context, Denial, KeyCache, ProofRequest, PublicKey, Replay, decode, verify_possession,
};
use ed25519_dalek::VerifyingKey;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Key {
    issuer: String,
    kid: String,
    x: String,
    until: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    issuer: String,
    keys: Vec<Key>,
    keyless_only: BTreeSet<String>,
    legacy_until: u64,
}
/// Explicit local authorization boundary, suitable for hosted request adapters.
/// Refresh replaces public snapshots atomically while retaining replay fences.
pub struct MachineGate {
    issuer: String,
    keys: KeyCache,
    replay: Replay,
    keyless_only: BTreeSet<String>,
    legacy_until: u64,
    quotas: BTreeMap<String, (u64, u32)>,
    pub unbound: u64,
}
impl MachineGate {
    pub fn new(manifest: &str, now: u64) -> Result<Self, Denial> {
        let mut gate = Self {
            issuer: String::new(),
            keys: KeyCache::new(64)?,
            replay: Replay::new(1024)?,
            keyless_only: BTreeSet::new(),
            legacy_until: 0,
            quotas: BTreeMap::new(),
            unbound: 0,
        };
        gate.refresh(manifest, now)?;
        Ok(gate)
    }
    /// Administrative/background only. This never discovers or fetches keys.
    pub fn refresh(&mut self, manifest: &str, now: u64) -> Result<(), Denial> {
        if manifest.len() > 131072 {
            return Err(Denial::Capacity);
        }
        let m: Manifest = serde_json::from_str(manifest).map_err(|_| Denial::Malformed)?;
        if m.issuer.is_empty()
            || m.issuer.len() > 2048
            || m.keys.is_empty()
            || m.keys.len() > 64
            || m.keyless_only.len() > 256
            || m.legacy_until > now.saturating_add(900)
        {
            return Err(Denial::Malformed);
        }
        let mut keys = KeyCache::new(64)?;
        let mut seen = BTreeSet::new();
        for k in m.keys {
            if k.issuer != m.issuer || !seen.insert(k.kid.clone()) {
                return Err(Denial::Malformed);
            }
            let bytes: [u8; 32] = decode(&k.x)?.try_into().map_err(|_| Denial::Malformed)?;
            keys.insert(
                PublicKey {
                    issuer: k.issuer,
                    kid: k.kid,
                    key: VerifyingKey::from_bytes(&bytes)
                        .map_err(|_| Denial::Malformed)?
                        .into(),
                    until: k.until,
                },
                now,
            )?;
        }
        self.keys = keys;
        self.issuer = m.issuer;
        self.keyless_only = m.keyless_only;
        self.legacy_until = m.legacy_until;
        Ok(())
    }
    pub fn issuer(&self) -> &str {
        &self.issuer
    }
    pub fn legacy(&mut self, tenant: &str, now: u64) -> Result<(), Denial> {
        if self.keyless_only.contains(tenant) || now >= self.legacy_until {
            return Err(Denial::Proof);
        }
        self.unbound += 1;
        Ok(())
    }
    /// Both validation stages must succeed before the caller evaluates a guard.
    pub fn admit(
        &mut self,
        request: &ProofRequest<'_>,
        proof: Option<&str>,
        tls_peer: Option<&str>,
        ctx: &Context<'_>,
        now: u64,
    ) -> Result<Claims, Denial> {
        let c = self.keys.validate(request.token, ctx, now)?;
        verify_possession(&c, request, proof, tls_peer, &mut self.replay, now)?;
        Ok(c)
    }
    pub fn quota(&mut self, tenant: &str, now: u64) -> Result<crate::auth::Permit, Denial> {
        let window = now / 60;
        self.quotas.retain(|_, (slot, _)| *slot >= window);
        if !self.quotas.contains_key(tenant) && self.quotas.len() >= 256 {
            return Err(Denial::Capacity);
        }
        let count = self.quotas.entry(tenant.into()).or_insert((window, 0));
        if count.1 >= 1000 {
            return Err(Denial::Limited);
        }
        count.1 += 1;
        Ok(crate::auth::Permit {
            tenant: tenant.into(),
            remaining: 1000 - count.1,
            limit: 1000,
            resets: count.0.saturating_add(1).saturating_mul(60),
        })
    }
}
