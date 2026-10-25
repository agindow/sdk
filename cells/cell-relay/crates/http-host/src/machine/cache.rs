use super::{
    Claims, Context, Denial, decode, hash,
    wire::{Header, parts, verify},
};
use ed25519_dalek::VerifyingKey;
use std::collections::{BTreeMap, VecDeque};
#[derive(Default, Debug)]
pub struct Counters {
    pub dropped: u64,
    pub stale: u64,
    pub full: u64,
    pub replayed: u64,
}
#[derive(Clone)]
pub enum VerificationKey {
    Ed25519(VerifyingKey),
    P256(p256::ecdsa::VerifyingKey),
}
impl From<VerifyingKey> for VerificationKey {
    fn from(key: VerifyingKey) -> Self {
        Self::Ed25519(key)
    }
}
impl From<p256::ecdsa::VerifyingKey> for VerificationKey {
    fn from(key: p256::ecdsa::VerifyingKey) -> Self {
        Self::P256(key)
    }
}
impl VerificationKey {
    pub(crate) fn verify(
        &self,
        algorithm: &str,
        signed: &str,
        signature: &str,
    ) -> Result<(), Denial> {
        match (self, algorithm) {
            (Self::Ed25519(key), "EdDSA") => verify(key, signed, signature),
            (Self::P256(key), "ES256") => {
                use p256::ecdsa::signature::Verifier;
                let signature = p256::ecdsa::Signature::from_slice(&decode(signature)?)
                    .map_err(|_| Denial::Signature)?;
                key.verify(signed.as_bytes(), &signature)
                    .map_err(|_| Denial::Signature)
            }
            _ => Err(Denial::Signature),
        }
    }
}
pub struct PublicKey {
    pub issuer: String,
    pub kid: String,
    pub key: VerificationKey,
    pub until: u64,
}
/// Bounded local issuer cache. Insert/refresh is a host background operation.
/// Unknown keys and stale snapshots never trigger request-path discovery.
pub struct KeyCache {
    entries: VecDeque<PublicKey>,
    capacity: usize,
    pub counters: Counters,
}
impl KeyCache {
    pub fn new(capacity: usize) -> Result<Self, Denial> {
        if !(1..=256).contains(&capacity) {
            return Err(Denial::Capacity);
        }
        Ok(Self {
            entries: VecDeque::new(),
            capacity,
            counters: Counters::default(),
        })
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub fn insert(&mut self, key: PublicKey, now: u64) -> Result<(), Denial> {
        if key.issuer.is_empty()
            || key.issuer.len() > 2048
            || key.kid.is_empty()
            || key.kid.len() > 256
            || key.until <= now
            || key.until > now.saturating_add(300)
        {
            return Err(Denial::Malformed);
        }
        self.entries
            .retain(|k| k.issuer != key.issuer || k.kid != key.kid);
        if self.entries.len() == self.capacity {
            self.entries.pop_front();
            self.counters.dropped += 1;
        }
        self.entries.push_back(key);
        Ok(())
    }
    pub fn remove(&mut self, issuer: &str, kid: &str) {
        self.entries.retain(|k| k.issuer != issuer || k.kid != kid);
    }
    pub(crate) fn key(
        &mut self,
        issuer: &str,
        kid: &str,
        now: u64,
    ) -> Result<VerificationKey, Denial> {
        let k = self
            .entries
            .iter()
            .find(|k| k.issuer == issuer && k.kid == kid)
            .ok_or(Denial::Unavailable)?;
        if k.until <= now {
            self.counters.stale += 1;
            return Err(Denial::Unavailable);
        }
        Ok(k.key.clone())
    }
    pub fn validate(&mut self, token: &str, ctx: &Context<'_>, now: u64) -> Result<Claims, Denial> {
        let (h, c, s, signed) = parts(token)?;
        let header: Header = serde_json::from_slice(&decode(h)?).map_err(|_| Denial::Malformed)?;
        if header.alg != "EdDSA" || header.typ != "at+jwt" {
            return Err(Denial::Signature);
        }
        let key = self.key(ctx.issuer, &header.kid, now)?;
        key.verify("EdDSA", signed, s)?;
        let claims: Claims = serde_json::from_slice(&decode(c)?).map_err(|_| Denial::Malformed)?;
        claims.check(ctx, now)?;
        Ok(claims)
    }
}
/// Saturation denies admission: evicting a live nonce would re-enable replay.
/// Nonces are stored only as digests. State is local to the hosting process.
pub struct Replay {
    entries: BTreeMap<String, u64>,
    capacity: usize,
    pub counters: Counters,
}
impl Replay {
    pub fn new(capacity: usize) -> Result<Self, Denial> {
        if !(1..=65536).contains(&capacity) {
            return Err(Denial::Capacity);
        }
        Ok(Self {
            entries: BTreeMap::new(),
            capacity,
            counters: Counters::default(),
        })
    }
    pub fn admit(
        &mut self,
        namespace: &str,
        nonce: &str,
        until: u64,
        now: u64,
    ) -> Result<(), Denial> {
        if nonce.is_empty() || nonce.len() > 256 || until <= now {
            return Err(Denial::Malformed);
        }
        self.entries.retain(|_, expiry| *expiry > now);
        let id = hash(format!("{}:{namespace}:{nonce}", namespace.len()).as_bytes());
        if self.entries.contains_key(&id) {
            self.counters.replayed += 1;
            return Err(Denial::Replay);
        }
        if self.entries.len() == self.capacity {
            self.counters.full += 1;
            return Err(Denial::Capacity);
        }
        self.entries.insert(id, until);
        Ok(())
    }
}
