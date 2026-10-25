use super::{CLOCK_SKEW_SECONDS, Denial, MAX_TOKEN_BYTES, MAX_TTL_SECONDS, decode, encode};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub enum Confirmation {
    #[serde(rename = "jkt")]
    Jkt(String),
    #[serde(rename = "x5t#S256")]
    Certificate(String),
}
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Actor {
    pub jti: String,
    pub sub: String,
    pub tenant: String,
    pub environment: String,
    pub aud: String,
    pub connection: String,
    pub tools: Vec<String>,
    pub exp: u64,
}
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Claims {
    pub iss: String,
    pub sub: String,
    pub aud: String,
    pub tenant: String,
    pub environment: String,
    pub connection: String,
    pub tools: Vec<String>,
    pub iat: u64,
    pub nbf: u64,
    pub exp: u64,
    pub jti: String,
    pub cnf: Confirmation,
    #[serde(rename = "agw_act")]
    pub act: Vec<Actor>,
}
/// Host-supplied route context: tenant/environment must come from its registered
/// binding, never the unverified request. Every field restricts authority.
pub struct Context<'a> {
    pub issuer: &'a str,
    pub tenant: &'a str,
    pub environment: &'a str,
    pub audience: &'a str,
    pub connection: &'a str,
    pub tool: &'a str,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Header {
    pub alg: String,
    pub typ: String,
    pub kid: String,
}
pub(crate) fn parts(token: &str) -> Result<(&str, &str, &str, &str), Denial> {
    if token.len() > MAX_TOKEN_BYTES {
        return Err(Denial::Malformed);
    }
    let (signed, signature) = token.rsplit_once('.').ok_or(Denial::Malformed)?;
    let (header, claims) = signed.split_once('.').ok_or(Denial::Malformed)?;
    if claims.contains('.') {
        return Err(Denial::Malformed);
    }
    Ok((header, claims, signature, signed))
}
pub(crate) fn verify(key: &VerifyingKey, signed: &str, signature: &str) -> Result<(), Denial> {
    let signature = Signature::from_slice(&decode(signature)?).map_err(|_| Denial::Signature)?;
    key.verify_strict(signed.as_bytes(), &signature)
        .map_err(|_| Denial::Signature)
}
pub(crate) fn jwt<T: Serialize>(
    claims: &T,
    typ: &str,
    kid: &str,
    key: &SigningKey,
) -> Result<String, Denial> {
    let header = Header {
        alg: "EdDSA".into(),
        typ: typ.into(),
        kid: kid.into(),
    };
    let signed = format!(
        "{}.{}",
        encode(serde_json::to_vec(&header).map_err(|_| Denial::Malformed)?),
        encode(serde_json::to_vec(claims).map_err(|_| Denial::Malformed)?)
    );
    let token = format!(
        "{signed}.{}",
        encode(key.sign(signed.as_bytes()).to_bytes())
    );
    if token.len() > MAX_TOKEN_BYTES {
        return Err(Denial::Capacity);
    }
    Ok(token)
}
/// Issuance signs only caller-owned claims; use Exchange to enforce authority.
pub fn sign(claims: &Claims, kid: &str, key: &SigningKey) -> Result<String, Denial> {
    jwt(claims, "at+jwt", kid, key)
}
pub(crate) fn literal(s: &str) -> bool {
    !s.is_empty() && s.len() <= 256 && !s.chars().any(|c| c.is_control() || c == '*' || c == '/')
}
pub(crate) fn tools_valid(tools: &[String]) -> bool {
    !tools.is_empty()
        && tools.len() <= 64
        && tools
            .iter()
            .enumerate()
            .all(|(i, t)| literal(t) && !tools[..i].contains(t))
}
impl Claims {
    pub(crate) fn check(&self, ctx: &Context<'_>, now: u64) -> Result<(), Denial> {
        if self.iss != ctx.issuer {
            return Err(Denial::Signature);
        }
        if !literal(&self.sub)
            || !literal(&self.jti)
            || !literal(&self.tenant)
            || !literal(&self.environment)
            || !literal(&self.connection)
            || !tools_valid(&self.tools)
        {
            return Err(Denial::Malformed);
        }
        let binding = match &self.cnf {
            Confirmation::Jkt(v) | Confirmation::Certificate(v) => v,
        };
        if decode(binding)?.len() != 32 {
            return Err(Denial::Malformed);
        }
        if self.exp <= self.iat
            || self.exp - self.iat > MAX_TTL_SECONDS
            || self.nbf > self.exp
            || self.iat > now.saturating_add(CLOCK_SKEW_SECONDS)
            || self.nbf > now.saturating_add(CLOCK_SKEW_SECONDS)
            || self.exp.saturating_add(CLOCK_SKEW_SECONDS) <= now
        {
            return Err(Denial::Expiry);
        }
        if self.aud != ctx.audience {
            return Err(Denial::Audience);
        }
        if self.tenant != ctx.tenant || self.environment != ctx.environment {
            return Err(Denial::Tenant);
        }
        if self.connection != ctx.connection || !self.tools.iter().any(|t| t == ctx.tool) {
            return Err(if self.act.is_empty() {
                Denial::Scope
            } else {
                Denial::Delegation(self.act.len())
            });
        }
        if self.act.len() > 8
            || serde_json::to_vec(&self.act)
                .map_err(|_| Denial::Malformed)?
                .len()
                > 8192
        {
            return Err(Denial::Capacity);
        }
        let mut ids = std::collections::BTreeSet::new();
        ids.insert(&self.jti);
        for (i, a) in self.act.iter().enumerate() {
            if a.tenant != self.tenant || a.environment != self.environment {
                return Err(Denial::Tenant);
            }
            if !literal(&a.jti)
                || !literal(&a.sub)
                || !tools_valid(&a.tools)
                || !ids.insert(&a.jti)
                || a.aud != self.aud
                || a.connection != self.connection
                || a.exp < self.exp
                || !self.tools.iter().all(|t| a.tools.contains(t))
                || (i > 0
                    && self.act.get(i - 1).is_some_and(|p| {
                        a.exp > p.exp || !a.tools.iter().all(|t| p.tools.contains(t))
                    }))
            {
                return Err(Denial::Delegation(i));
            }
            if a.exp.saturating_add(CLOCK_SKEW_SECONDS) <= now {
                return Err(Denial::Expiry);
            }
        }
        Ok(())
    }
}
