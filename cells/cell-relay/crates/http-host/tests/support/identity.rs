#![allow(dead_code)] // Shared fixtures used by independent integration suites.
use agw_http_host::machine::{Claims, Confirmation, Context, KeyCache, PublicKey, sign};
use ed25519_dalek::SigningKey;
pub const NOW: u64 = 1_000;
pub fn key() -> SigningKey {
    SigningKey::from_bytes(&[7; 32])
}
pub fn claims() -> Claims {
    Claims {
        iss: "https://agindow.test".into(),
        sub: "workload".into(),
        aud: "agindow:guard".into(),
        tenant: "a".into(),
        environment: "test".into(),
        connection: "c".into(),
        tools: vec!["read".into(), "write".into()],
        iat: NOW,
        nbf: NOW,
        exp: NOW + 300,
        jti: "access-1".into(),
        cnf: Confirmation::Jkt(agw_http_host::machine::thumbprint(&key().verifying_key())),
        act: vec![],
    }
}
pub fn context() -> Context<'static> {
    Context {
        issuer: "https://agindow.test",
        tenant: "a",
        environment: "test",
        audience: "agindow:guard",
        connection: "c",
        tool: "read",
    }
}
pub fn cache() -> Result<KeyCache, agw_http_host::machine::Denial> {
    let mut cache = KeyCache::new(2)?;
    cache.insert(
        PublicKey {
            issuer: "https://agindow.test".into(),
            kid: "one".into(),
            key: key().verifying_key().into(),
            until: NOW + 300,
        },
        NOW,
    )?;
    Ok(cache)
}
pub fn token(c: &Claims) -> Result<String, agw_http_host::machine::Denial> {
    sign(c, "one", &key())
}
