use crate::{
    Envelope,
    auth::{Auth, Permit, Refusal},
    execution::Execution,
};
use agw_guard::statistical::{Engine, Operation};
use serde::Deserialize;
use std::{cell::RefCell, collections::BTreeMap};
use worker::wasm_bindgen::JsCast;
use worker::{Env, Error};

#[derive(Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct Route {
    pub tenant: String,
    #[serde(default = "environment")]
    pub environment: String,
    pub url: String,
    pub method: String,
    pub operation: Operation,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub selection: serde_json::Value,
}
struct Cache {
    auth_json: String,
    routes_json: String,
    auth: Option<Auth>,
    routes: BTreeMap<String, Route>,
    engines: BTreeMap<String, Engine>,
    sequence: u64,
}
thread_local! { static CACHE: RefCell<Option<Cache>> = const { RefCell::new(None) }; }
pub struct Prepared {
    pub route: Route,
    pub permit: Permit,
    pub call: Option<Execution>,
}
fn environment() -> String {
    "live".into()
}
pub fn prepare(
    env: &Env,
    input: &Envelope,
    token_value: &str,
    proof: Option<&str>,
    tls_peer: Option<&str>,
    target: &str,
) -> Result<Prepared, u16> {
    let auth_json = env
        .secret("GUARD_KEYS")
        .ok()
        .map(|v| v.to_string())
        .unwrap_or_default();
    let routes_json = env
        .secret("GUARD_CONNECTIONS")
        .map_err(|_| 503u16)?
        .to_string();
    if auth_json.len() > 262144 || routes_json.len() > 4 * 1024 * 1024 {
        return Err(503);
    }
    let now = js_sys::Date::now();
    if !now.is_finite() || now < 0.0 {
        return Err(503);
    }
    let at = chrono::DateTime::from_timestamp_millis(now as i64).ok_or(503u16)?;
    agw_guard::readiness::advance_host_time(std::time::Duration::from_secs_f64(now / 1000.0));
    CACHE.with(|cell| {
        let mut slot = cell.try_borrow_mut().map_err(|_| 503u16)?;
        if slot
            .as_ref()
            .is_none_or(|cache| cache.auth_json != auth_json || cache.routes_json != routes_json)
        {
            let auth = if auth_json.is_empty() {
                None
            } else {
                Some(Auth::new(&auth_json).map_err(|_| 503u16)?)
            };
            let routes: BTreeMap<String, Route> =
                serde_json::from_str(&routes_json).map_err(|_| 503u16)?;
            if routes.is_empty() || routes.len() > 256 {
                return Err(503);
            }
            let mut engines = BTreeMap::new();
            for (key, route) in &routes {
                if !route.url.starts_with("https://")
                    || route.tenant.is_empty()
                    || route.headers.len() > 32
                {
                    return Err(503);
                }
                let url = worker::Url::parse(&route.url).map_err(|_| 503u16)?;
                if !url.username().is_empty()
                    || url.password().is_some()
                    || url.fragment().is_some()
                {
                    return Err(503);
                }
                if !route.selection.is_null()
                    && (route.selection["tenant"] != route.tenant
                        || route.selection["connection"].as_str() != key.split('/').next()
                        || route.selection["operation"].as_str() != key.split('/').nth(1))
                {
                    return Err(503);
                }
                engines.insert(
                    key.clone(),
                    agw_node_guest::local_engine(&route.selection.to_string())
                        .map_err(|_| 503u16)?,
                );
            }
            *slot = Some(Cache {
                auth_json,
                routes_json,
                auth,
                routes,
                engines,
                sequence: 0,
            });
        }
        let cache = slot.as_mut().ok_or(503u16)?;
        let key = format!("{}/{}", input.connection, input.tool);
        let route = cache.routes.get(&key).ok_or(401u16)?.clone();
        if input.operation != route.operation {
            return Err(400);
        }
        let machine = super::identity::admit(
            env,
            &super::identity::Scope {
                tenant: &route.tenant,
                environment: &route.environment,
                connection: &input.connection,
                tool: &input.tool,
            },
            &super::identity::RequestAuth {
                tls_peer,
                method: "POST",
                authorization: token_value,
                proof,
                target,
            },
            (now / 1000.0) as u64,
        )?;
        let authority = machine.as_ref().map(|a| a.claims.clone());
        let permit = if let Some(accepted) = machine {
            accepted.permit
        } else {
            cache
                .auth
                .as_mut()
                .ok_or(401u16)?
                .admit(token_value, &key, (now / 1000.0) as u64)
                .map_err(|error| match error {
                    Refusal::Unauthorized => 401u16,
                    Refusal::Scope => 403u16,
                    Refusal::Limited => 429u16,
                })?
        };
        if permit.tenant != route.tenant {
            return Err(403);
        }
        cache.sequence = cache.sequence.checked_add(1).ok_or(503u16)?;
        let crypto =
            js_sys::Reflect::get(&js_sys::global(), &"crypto".into()).map_err(|_| 503u16)?;
        let random = js_sys::Reflect::get(&crypto, &"randomUUID".into())
            .map_err(|_| 503u16)?
            .dyn_into::<js_sys::Function>()
            .map_err(|_| 503u16)?;
        let id = format!(
            "aud_{}",
            random
                .call0(&crypto)
                .map_err(|_| 503u16)?
                .as_string()
                .ok_or(503u16)?
        );
        let engine = cache.engines.get(&key).ok_or(503u16)?;
        let call = Execution::start_at(input, engine, id, 200_000_000, at)
            .ok()
            .and_then(|call| match authority {
                Some(c) => call.with_machine_authority(&c).ok(),
                None => Some(call),
            });
        Ok(Prepared {
            route,
            permit,
            call,
        })
    })
}
pub fn internal() -> Error {
    Error::RustError("hosted guard unavailable".into())
}
