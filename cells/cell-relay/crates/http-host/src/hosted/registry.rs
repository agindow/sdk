//! Explicit administrator probe, completely outside the guard decision path.
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use worker::{Env, Fetcher, Request, Response, Result};
pub async fn probe(request: &Request, env: &Env) -> Result<Response> {
    let secret = env.secret("REGISTRY_PROBE_KEY")?.to_string();
    let token = request.headers().get("authorization")?.unwrap_or_default();
    let actual: [u8; 32] = Sha256::digest(token.as_bytes()).into();
    let expected: [u8; 32] = Sha256::digest(format!("Bearer {secret}").as_bytes()).into();
    if secret.is_empty() || !bool::from(actual.ct_eq(&expected)) {
        return Response::error("Unauthorized", 401);
    }
    let url = env.var("REGISTRY_HEALTH_URL")?.to_string();
    let parsed = worker::Url::parse(&url)?;
    if parsed.scheme() != "https" || !parsed.username().is_empty() || parsed.password().is_some() {
        return Response::error("Registry config unavailable", 503);
    }
    let binding: Fetcher = env.get_binding("REGISTRY_MTLS")?;
    let result = binding.fetch(url, None).await?;
    Response::from_json(
        &serde_json::json!({"mtls_binding":"REGISTRY_MTLS","registry_status":result.status_code()}),
    )
}
