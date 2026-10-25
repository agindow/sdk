use super::{
    Claims, Confirmation, Denial, KeyCache, MAX_TTL_SECONDS, Replay, decode, sign,
    wire::{jwt, parts, tools_valid},
};
use ed25519_dalek::SigningKey;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
/// Trusted administrator registration. Caller input may only select its ID.
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Registration {
    pub id: String,
    pub kid: String,
    pub audiences: Vec<String>,
    pub issuer: String,
    pub subject: String,
    pub audience: String,
    pub tenant: String,
    pub environment: String,
    pub connection: String,
    pub tools: Vec<String>,
    pub current: bool,
    pub private_key_jwt: bool,
}
#[derive(Deserialize, Serialize)]
pub struct SubjectClaims {
    pub iss: String,
    pub sub: String,
    pub aud: String,
    pub iat: u64,
    pub nbf: u64,
    pub exp: u64,
    pub jti: Option<String>,
}
pub fn sign_subject(claims: &SubjectClaims, kid: &str, key: &SigningKey) -> Result<String, Denial> {
    jwt(claims, "JWT", kid, key)
}
pub struct ExchangeRequest<'a> {
    pub registration: &'a str,
    pub assertion: &'a str,
    pub audience: &'a str,
    pub tools: Vec<String>,
    pub ttl: u64,
    pub cnf: Confirmation,
    pub subject: &'a str,
}
/// Control-plane component; no acquisition/discovery belongs in guard calls.
/// No Debug implementation: the issuer private signing key stays opaque.
pub struct Exchange {
    pub(crate) instance: String,
    pub(crate) issuer: String,
    pub(crate) kid: String,
    pub(crate) key: SigningKey,
    registrations: BTreeMap<String, Registration>,
    replay: Replay,
    pub(crate) sequence: u64,
    pub(crate) rates: BTreeMap<String, (u64, u32)>,
    pub(crate) capacity: usize,
    pub limited: u64,
}
impl Exchange {
    pub fn new(
        issuer: String,
        kid: String,
        key: SigningKey,
        capacity: usize,
        instance: String,
    ) -> Result<Self, Denial> {
        if instance.is_empty()
            || instance.len() > 256
            || issuer.is_empty()
            || kid.is_empty()
            || !(1..=4096).contains(&capacity)
        {
            return Err(Denial::Capacity);
        }
        Ok(Self {
            instance,
            issuer,
            kid,
            key,
            registrations: BTreeMap::new(),
            replay: Replay::new(1024)?,
            sequence: 0,
            rates: BTreeMap::new(),
            capacity,
            limited: 0,
        })
    }
    pub fn register(&mut self, r: Registration) -> Result<(), Denial> {
        if r.kid.is_empty()
            || r.audiences.is_empty()
            || r.audiences.len() > 4
            || r.id.is_empty()
            || r.id.len() > 256
            || r.issuer.len() > 2048
            || r.subject.is_empty()
            || r.subject.len() > 2048
            || r.audience.is_empty()
            || !tools_valid(&r.tools)
            || !super::wire::literal(&r.tenant)
            || !super::wire::literal(&r.environment)
            || !super::wire::literal(&r.connection)
            || (r.private_key_jwt && r.issuer != r.subject)
        {
            return Err(Denial::Registration);
        }
        if !self.registrations.contains_key(&r.id) && self.registrations.len() >= self.capacity {
            return Err(Denial::Capacity);
        }
        self.registrations.insert(r.id.clone(), r);
        Ok(())
    }
    pub fn reconfigure(
        &mut self,
        issuer: String,
        kid: String,
        key: SigningKey,
        registrations: Vec<Registration>,
    ) -> Result<(), Denial> {
        let mut candidate = Self::new(issuer, kid, key, self.capacity, self.instance.clone())?;
        for r in registrations {
            candidate.register(r)?;
        }
        self.issuer = candidate.issuer;
        self.kid = candidate.kid;
        self.key = candidate.key;
        self.registrations = candidate.registrations;
        Ok(())
    }
    pub fn remove(&mut self, id: &str) {
        self.registrations.remove(id);
    }
    pub(crate) fn next_id(&mut self, now: u64) -> Result<String, Denial> {
        self.sequence = self.sequence.checked_add(1).ok_or(Denial::Capacity)?;
        // Derive IDs from a signature: unique across issuer keys and process starts
        // when the host supplies a fresh signing key or persists its sequence.
        use ed25519_dalek::Signer;
        Ok(super::hash(
            &self
                .key
                .sign(format!("{}:{now}:{}", self.instance, self.sequence).as_bytes())
                .to_bytes(),
        ))
    }
    pub(crate) fn rate(&mut self, subject: &str, now: u64) -> Result<(), Denial> {
        let window = now / 60;
        self.rates.retain(|_, (slot, _)| *slot >= window);
        if !self.rates.contains_key(subject) && self.rates.len() >= self.capacity {
            self.limited += 1;
            return Err(Denial::Capacity);
        }
        let count = self.rates.entry(subject.into()).or_insert((window, 0));
        if count.1 >= 60 {
            self.limited += 1;
            return Err(Denial::Limited);
        }
        count.1 += 1;
        Ok(())
    }
    pub fn exchange(
        &mut self,
        req: &ExchangeRequest<'_>,
        keys: &mut KeyCache,
        now: u64,
    ) -> Result<String, Denial> {
        let r = self
            .registrations
            .get(req.registration)
            .ok_or(Denial::Registration)?
            .clone();
        if !r.current {
            return Err(Denial::Registration);
        }
        if !r.audiences.iter().any(|a| a == req.audience)
            || !matches!(
                req.audience,
                "agindow:guard"
                    | "agindow:registry:read"
                    | "agindow:registry:write"
                    | "agindow:mcp"
            )
        {
            return Err(Denial::Audience);
        }
        if !tools_valid(&req.tools) || !req.tools.iter().all(|t| r.tools.contains(t)) {
            return Err(Denial::Scope);
        }
        if req.ttl == 0 || req.ttl > MAX_TTL_SECONDS {
            return Err(Denial::Expiry);
        }
        let (h, c, s, signed) = parts(req.assertion)?;
        let header: super::assertion::AssertionHeader =
            serde_json::from_slice(&decode(h)?).map_err(|_| Denial::Malformed)?;
        let spiffe = r.subject.starts_with("spiffe://");
        let kid = header.kid.as_deref().unwrap_or(&r.kid);
        if kid != r.kid
            || !matches!(header.alg.as_str(), "EdDSA" | "ES256")
            || header
                .typ
                .as_deref()
                .is_some_and(|t| t != "JWT" && (!spiffe || t != "JOSE"))
            || (spiffe && header.alg != "ES256")
        {
            return Err(Denial::Signature);
        }
        let key = keys.key(&r.issuer, kid, now)?;
        key.verify(&header.alg, signed, s)?;
        let c: super::assertion::Subject =
            serde_json::from_slice(&decode(c)?).map_err(|_| Denial::Malformed)?;
        c.check(&r, now)?;
        if r.private_key_jwt {
            self.replay.admit(
                &r.id,
                c.jti.as_deref().ok_or(Denial::Malformed)?,
                c.replay_until(),
                now,
            )?;
        }
        self.rate(&r.id, now)?;
        let c = Claims {
            iss: self.issuer.clone(),
            sub: r.subject,
            aud: req.audience.into(),
            tenant: r.tenant,
            environment: r.environment,
            connection: r.connection,
            tools: req.tools.clone(),
            iat: now,
            nbf: now,
            exp: now.checked_add(req.ttl).ok_or(Denial::Expiry)?,
            jti: self.next_id(now)?,
            cnf: req.cnf.clone(),
            act: vec![],
        };
        // A workload's external subject may be a URI; access subject is opaque.
        let mut c = c;
        c.sub = super::hash(c.sub.as_bytes());
        let ctx = super::Context {
            issuer: &c.iss,
            tenant: &c.tenant,
            environment: &c.environment,
            audience: &c.aud,
            connection: &c.connection,
            tool: c.tools.first().ok_or(Denial::Scope)?,
        };
        c.check(&ctx, now)?;
        sign(&c, &self.kid, &self.key)
    }
}
