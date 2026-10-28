//! Per-execution-owner keys rotate on DO activation AND WASM reincarnation.
use crate::machine::{Denial, OwnerEpoch};
use std::{cell::RefCell, collections::BTreeMap};
use worker::{Env, wasm_bindgen::JsCast};
thread_local! { static EPOCHS:RefCell<BTreeMap<String,OwnerEpoch>>=const{RefCell::new(BTreeMap::new())}; }
fn with_epoch<T>(
    env: &Env,
    work: impl FnOnce(&OwnerEpoch) -> Result<T, Denial>,
) -> Result<Option<T>, Denial> {
    let Ok(id) = env.var("GUARD_OWNER_ID") else {
        return Ok(None);
    };
    let id = id.to_string();
    let (owner, activation) = id.split_once(':').ok_or(Denial::Malformed)?;
    if activation.len() != 36
        || !activation
            .bytes()
            .all(|b| b.is_ascii_hexdigit() || b == b'-')
    {
        return Err(Denial::Malformed);
    }
    EPOCHS.with(|cell| {
        let mut epochs = cell.try_borrow_mut().map_err(|_| Denial::Unavailable)?;
        if !epochs.contains_key(&id) {
            if epochs.len() >= 256 {
                return Err(Denial::Capacity);
            }
            let crypto = js_sys::Reflect::get(&js_sys::global(), &"crypto".into())
                .map_err(|_| Denial::Unavailable)?;
            let random = js_sys::Reflect::get(&crypto, &"getRandomValues".into())
                .map_err(|_| Denial::Unavailable)?
                .dyn_into::<js_sys::Function>()
                .map_err(|_| Denial::Unavailable)?;
            let bytes = js_sys::Uint8Array::new_with_length(32);
            random
                .call1(&crypto, &bytes)
                .map_err(|_| Denial::Unavailable)?;
            let mut seed = [0u8; 32];
            bytes.copy_to(&mut seed);
            epochs.insert(id.clone(), OwnerEpoch::new(owner, seed)?);
        }
        work(epochs.get(&id).ok_or(Denial::Unavailable)?).map(Some)
    })
}
pub fn manifest(env: &Env, raw: &str, now: u64) -> Result<Option<String>, Denial> {
    with_epoch(env, |epoch| {
        let value: serde_json::Value = serde_json::from_str(raw).map_err(|_| Denial::Malformed)?;
        let issuer = value["issuer"].as_str().ok_or(Denial::Malformed)?;
        let tenants: Vec<String> =
            serde_json::from_value(value["keyless_only"].clone()).map_err(|_| Denial::Malformed)?;
        epoch.manifest(issuer, &tenants, now)
    })
}
pub fn exchange(env: &Env, raw: &str, now: u64) -> Result<Option<String>, Denial> {
    with_epoch(env, |epoch| epoch.exchange_config(raw, now))
}

/// Trusted host binding, never a token/header hint. Standalone hosts retain one namespace.
pub fn namespace(env: &Env) -> String {
    env.var("GUARD_OWNER_ID")
        .map(|id| id.to_string())
        .unwrap_or_default()
}
