//! Opt-in public-key snapshot from host configuration. No discovery in calls.
use crate::{
    auth::Permit,
    machine::{Claims, Context, Denial, MachineGate, ProofRequest},
};
use std::cell::RefCell;
use worker::Env;
struct Cached {
    manifest: String,
    gate: MachineGate,
}
thread_local! { static CACHE:RefCell<Option<Cached>>=const { RefCell::new(None) };
static DENIAL:RefCell<Option<Denial>>=const{RefCell::new(None)}; }
pub struct RequestAuth<'a> {
    pub tls_peer: Option<&'a str>,
    pub method: &'a str,
    pub authorization: &'a str,
    pub proof: Option<&'a str>,
    pub target: &'a str,
}
pub struct Scope<'a> {
    pub tenant: &'a str,
    pub environment: &'a str,
    pub connection: &'a str,
    pub tool: &'a str,
}
pub struct Accepted {
    pub permit: Permit,
    pub claims: Claims,
}
pub fn denial() -> Option<String> {
    DENIAL.with(|d| d.borrow_mut().take().map(|r| format!("auth_{r:?}")))
}
pub fn admit(
    env: &Env,
    scope: &Scope<'_>,
    auth: &RequestAuth<'_>,
    now: u64,
) -> Result<Option<Accepted>, u16> {
    DENIAL.with(|d| *d.borrow_mut() = None);
    let Some(manifest) = env.secret("GUARD_MACHINE_AUTH").ok().map(|v| v.to_string()) else {
        return Ok(None);
    };
    CACHE.with(|cell| {
        let mut slot = cell.try_borrow_mut().map_err(|_| 503u16)?;
        if let Some(c) = slot.as_mut() {
            if c.manifest != manifest {
                c.gate
                    .refresh(&manifest, now)
                    .map_err(|_| status(Denial::Unavailable))?;
                c.manifest = manifest;
            }
        } else {
            *slot = Some(Cached {
                gate: MachineGate::new(&manifest, now).map_err(|_| status(Denial::Unavailable))?,
                manifest,
            });
        }
        let c = slot.as_mut().ok_or(503u16)?;
        let dpop = auth.authorization.strip_prefix("DPoP ");
        let mtls = auth
            .authorization
            .strip_prefix("Bearer ")
            .filter(|t| t.contains('.'));
        if let Some(token) = dpop.or(mtls) {
            let issuer = c.gate.issuer().to_string();
            let context = Context {
                issuer: &issuer,
                tenant: scope.tenant,
                environment: scope.environment,
                audience: "agindow:guard",
                connection: scope.connection,
                tool: scope.tool,
            };
            let claims = c
                .gate
                .admit(
                    &ProofRequest {
                        method: auth.method,
                        target: auth.target,
                        token,
                    },
                    auth.proof,
                    if mtls.is_some() { auth.tls_peer } else { None },
                    &context,
                    now,
                )
                .map_err(status)?;
            return c
                .gate
                .quota(scope.tenant, now)
                .map(|permit| Some(Accepted { permit, claims }))
                .map_err(status);
        }
        c.gate.legacy(scope.tenant, now).map_err(status)?;
        Ok(None)
    })
}
fn status(reason: Denial) -> u16 {
    DENIAL.with(|d| *d.borrow_mut() = Some(reason.clone()));
    match reason {
        Denial::Unavailable => 503,
        Denial::Tenant | Denial::Scope | Denial::Delegation(_) => 403,
        Denial::Limited | Denial::Capacity => 429,
        _ => 401,
    }
}

/// Cloudflare-attested metadata only. A request header is never a TLS peer.
pub fn tls_peer(request: &worker::Request) -> Option<String> {
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    let auth = request.cf()?.tls_client_auth()?;
    if auth.cert_verified() != "SUCCESS" {
        return None;
    }
    let hex = auth.cert_fingerprint_sha256().replace(':', "");
    if hex.len() != 64 || !hex.is_ascii() {
        return None;
    }
    let mut bytes = [0u8; 32];
    for (i, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(URL_SAFE_NO_PAD.encode(bytes))
}
