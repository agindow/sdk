//! Local exchange HTTP adapter. Host secrets are administrative configuration;
//! there is no registration endpoint and requests cannot name a tenant.
use crate::machine::{
    Confirmation, Context, DelegateRequest, Denial, Exchange, ExchangeRequest, KeyCache, PublicKey,
    Registration, Replay,
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use ed25519_dalek::{SigningKey, VerifyingKey};
use futures_util::StreamExt;
use serde::Deserialize;
use std::{cell::RefCell, collections::BTreeMap};
use worker::{Env, Request, Response, Result, wasm_bindgen::JsCast};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProviderKey {
    issuer: String,
    kid: String,
    x: String,
    #[serde(default)]
    y: Option<String>,
    until: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    issuer: String,
    kid: String,
    signing_seed: String,
    keys: Vec<ProviderKey>,
    registrations: Vec<Registration>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    grant_type: String,
    subject_token_type: String,
    subject_token: String,
    #[serde(default)]
    registration: String,
    #[serde(default)]
    connection: Option<String>,
    #[serde(default)]
    subject: Option<String>,
    audience: String,
    tools: Vec<String>,
    ttl: u64,
    cnf: Confirmation,
}
struct Cached {
    config: String,
    exchange: Exchange,
    keys: KeyCache,
    replay: Replay,
}
thread_local! {static CACHE:RefCell<BTreeMap<String,Option<Cached>>>=const{RefCell::new(BTreeMap::new())};}
fn nonce() -> std::result::Result<String, Denial> {
    let crypto = js_sys::Reflect::get(&js_sys::global(), &"crypto".into())
        .map_err(|_| Denial::Unavailable)?;
    let random = js_sys::Reflect::get(&crypto, &"randomUUID".into())
        .map_err(|_| Denial::Unavailable)?
        .dyn_into::<js_sys::Function>()
        .map_err(|_| Denial::Unavailable)?;
    random
        .call0(&crypto)
        .map_err(|_| Denial::Unavailable)?
        .as_string()
        .ok_or(Denial::Unavailable)
}
fn decode<const N: usize>(value: &str) -> std::result::Result<[u8; N], Denial> {
    URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| Denial::Malformed)?
        .try_into()
        .map_err(|_| Denial::Malformed)
}
fn prepare(
    slot: &mut Option<Cached>,
    config: String,
    now: u64,
) -> std::result::Result<&mut Cached, Denial> {
    if slot.as_ref().is_none_or(|c| c.config != config) {
        if config.len() > 262144 {
            return Err(Denial::Unavailable);
        }
        let cfg: Config = serde_json::from_str(&config).map_err(|_| Denial::Unavailable)?;
        if cfg.keys.len() > 64 || cfg.registrations.len() > 256 {
            return Err(Denial::Unavailable);
        }
        let mut keys = KeyCache::new(64)?;
        for k in cfg.keys {
            keys.insert(
                PublicKey {
                    issuer: k.issuer,
                    kid: k.kid,
                    key: if let Some(y) = k.y {
                        let mut sec = [0u8; 65];
                        sec[0] = 4;
                        sec[1..33].copy_from_slice(&decode::<32>(&k.x)?);
                        sec[33..].copy_from_slice(&decode::<32>(&y)?);
                        p256::ecdsa::VerifyingKey::from_sec1_bytes(&sec)
                            .map_err(|_| Denial::Unavailable)?
                            .into()
                    } else {
                        VerifyingKey::from_bytes(&decode(&k.x)?)
                            .map_err(|_| Denial::Unavailable)?
                            .into()
                    },
                    until: k.until,
                },
                now,
            )?;
        }
        let key = SigningKey::from_bytes(&decode(&cfg.signing_seed)?);
        if let Some(c) = slot.as_mut() {
            c.exchange
                .reconfigure(cfg.issuer, cfg.kid, key, cfg.registrations)?;
            c.keys = keys;
            c.config = config;
        } else {
            let mut exchange = Exchange::new(cfg.issuer, cfg.kid, key, 256, nonce()?)?;
            for r in cfg.registrations {
                exchange.register(r)?;
            }
            *slot = Some(Cached {
                replay: Replay::new(1024)?,
                config,
                exchange,
                keys,
            });
        }
    }
    slot.as_mut().ok_or(Denial::Unavailable)
}
pub async fn handle(request: &mut Request, env: &Env) -> Result<Response> {
    let config = match env.secret("GUARD_IDENTITY_EXCHANGE") {
        Ok(c) => c.to_string(),
        Err(_) => return super::error(503, "identity_unavailable"),
    };
    let mut body = Vec::new();
    let mut stream = request.stream()?;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        if body.len().saturating_add(chunk.len()) > 32768 {
            return super::error(413, "invalid_request");
        }
        body.extend_from_slice(&chunk);
    }
    let input: Input = match serde_json::from_slice(&body) {
        Ok(i) => i,
        Err(_) => return super::error(400, "invalid_request"),
    };
    if input.grant_type != "urn:ietf:params:oauth:grant-type:token-exchange"
        || !matches!(
            input.subject_token_type.as_str(),
            "urn:ietf:params:oauth:token-type:jwt"
                | "urn:ietf:params:oauth:token-type:access_token"
        )
    {
        return super::error(400, "unsupported_grant_type");
    }
    let now = (js_sys::Date::now() / 1000.0) as u64;
    let config = match super::owner::exchange(env, &config, now) {
        Ok(value) => value.unwrap_or(config),
        Err(_) => return super::error(503, "identity_unavailable"),
    };
    let proof = request.headers().get("dpop")?;
    let target = request.url()?;
    let routes = env.secret("GUARD_CONNECTIONS").ok().map(|v| v.to_string());
    let token_type = if matches!(&input.cnf, Confirmation::Jkt(_)) {
        "DPoP"
    } else {
        "Bearer"
    };
    let result = CACHE.with(|cell| {
        let mut slots = cell.try_borrow_mut().map_err(|_| Denial::Unavailable)?;
        let namespace = super::owner::namespace(env);
        if !slots.contains_key(&namespace) && slots.len() >= 256 {
            return Err(Denial::Capacity);
        }
        let slot = slots.entry(namespace).or_default();
        let c = prepare(slot, config, now).map_err(|_| Denial::Unavailable)?;
        if input.subject_token_type == "urn:ietf:params:oauth:token-type:access_token" {
            if input.ttl == 0 || input.ttl > 900 {
                return Err(Denial::Expiry);
            }
            let connection = input.connection.as_deref().ok_or(Denial::Scope)?;
            let tool = input.tools.first().ok_or(Denial::Scope)?.clone();
            let raw = routes.as_deref().ok_or(Denial::Unavailable)?;
            if raw.len() > 4 * 1024 * 1024 {
                return Err(Denial::Unavailable);
            }
            let bindings: std::collections::BTreeMap<String, super::state::Route> =
                serde_json::from_str(raw).map_err(|_| Denial::Unavailable)?;
            let route = bindings
                .get(&format!("{connection}/{tool}"))
                .ok_or(Denial::Scope)?;
            let issuer = c.exchange.issuer.clone();
            let context = Context {
                issuer: &issuer,
                tenant: &route.tenant,
                environment: &route.environment,
                audience: &input.audience,
                connection,
                tool: &tool,
            };
            return c.exchange.delegate(
                &DelegateRequest {
                    parent: &input.subject_token,
                    context,
                    proof: proof.as_deref(),
                    tls_peer: None,
                    target: target.as_str(),
                    tools: input.tools,
                    exp: now.checked_add(input.ttl).ok_or(Denial::Expiry)?,
                    subject: input.subject.as_deref().unwrap_or("delegate"),
                    cnf: input.cnf,
                    max_depth: 3,
                },
                &mut c.keys,
                &mut c.replay,
                now,
            );
        }
        c.exchange.exchange(
            &ExchangeRequest {
                registration: &input.registration,
                assertion: &input.subject_token,
                audience: &input.audience,
                tools: input.tools,
                ttl: input.ttl,
                cnf: input.cnf,
                subject: "workload",
            },
            &mut c.keys,
            now,
        )
    });
    match result {
        Ok(token) => {
            let response = Response::from_json(
                &serde_json::json!({"access_token":token,"token_type":token_type,"issued_token_type":"urn:ietf:params:oauth:token-type:access_token","expires_in":input.ttl,"expires_at":now+input.ttl}),
            )?;
            response.headers().set("cache-control", "no-store")?;
            response.headers().set("pragma", "no-cache")?;
            Ok(response)
        }
        Err(reason) => super::error(
            match reason {
                Denial::Unavailable => 503,
                Denial::Scope | Denial::Tenant | Denial::Registration => 403,
                Denial::Limited | Denial::Capacity => 429,
                _ => 401,
            },
            &format!("identity_{reason:?}"),
        ),
    }
}
