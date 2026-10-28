//! Signing authority and proof fences must have the same lifetime. No key is persisted.
use super::{Denial, encode, hash};
use ed25519_dalek::SigningKey;
use serde_json::{Value, json};
pub struct OwnerEpoch {
    kid: String,
    key: SigningKey,
}
impl OwnerEpoch {
    pub fn new(owner: &str, seed: [u8; 32]) -> Result<Self, Denial> {
        if owner.len() != 64 || !owner.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(Denial::Malformed);
        }
        let key = SigningKey::from_bytes(&seed);
        let kid = format!("owner.{owner}.{}", hash(key.verifying_key().as_bytes()));
        Ok(Self { kid, key })
    }
    pub fn kid(&self) -> &str {
        &self.kid
    }
    fn public(&self, issuer: &str, now: u64) -> Value {
        json!({"issuer":issuer,"kid":self.kid,"x":encode(self.key.verifying_key().as_bytes()),"until":now.saturating_add(300)})
    }
    pub fn manifest(&self, issuer: &str, tenants: &[String], now: u64) -> Result<String, Denial> {
        if issuer.is_empty() || tenants.is_empty() || tenants.len() > 256 {
            return Err(Denial::Malformed);
        }
        Ok(json!({"issuer":issuer,"keys":[self.public(issuer,now)],"keyless_only":tenants,"legacy_until":0}).to_string())
    }
    /// Private host configuration only; never return this value over HTTP or log it.
    pub fn exchange_config(&self, raw: &str, now: u64) -> Result<String, Denial> {
        if raw.len() > 262144 {
            return Err(Denial::Capacity);
        }
        let mut config: Value = serde_json::from_str(raw).map_err(|_| Denial::Malformed)?;
        let issuer = config
            .get("issuer")
            .and_then(Value::as_str)
            .ok_or(Denial::Malformed)?
            .to_string();
        let object = config.as_object_mut().ok_or(Denial::Malformed)?;
        let keys = object
            .get_mut("keys")
            .and_then(Value::as_array_mut)
            .ok_or(Denial::Malformed)?;
        if keys.len() >= 64 {
            return Err(Denial::Capacity);
        }
        keys.push(self.public(&issuer, now));
        object.insert("kid".into(), json!(self.kid));
        object.insert("signing_seed".into(), json!(encode(self.key.to_bytes())));
        Ok(config.to_string())
    }
}
