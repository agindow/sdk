//! TI-34 workers-rs host. Registry mTLS is an explicit administrative call,
//! never a synchronous guard dependency. No hosted qualification is implied.
mod archive;
mod exchange;
mod identity;
mod owner;
mod registry;
mod state;
use crate::{
    Envelope,
    auth::Permit,
    execution::{Completed, Execution},
};
use futures_util::{StreamExt, stream};
use worker::{Context, Env, Fetch, Headers, Method, Request, RequestInit, Response, Result, event};
const CAP: usize = 8 * 1024 * 1024;

#[event(fetch)]
pub async fn fetch(mut request: Request, env: Env, _context: Context) -> Result<Response> {
    match handle(&mut request, &env).await {
        Ok(response) => Ok(response),
        Err(_) => error(503, "internal"),
    }
}
fn error(status: u16, code: &str) -> Result<Response> {
    Ok(Response::from_json(&serde_json::json!({"error":code}))?.with_status(status))
}
async fn handle(request: &mut Request, env: &Env) -> Result<Response> {
    if request.path() == "/v1/token" && request.method() == Method::Post {
        return exchange::handle(request, env).await;
    }
    if request.path() == "/internal/registry-probe" && request.method() == Method::Post {
        return registry::probe(request, env).await;
    }
    for (prefix, outcome) in [("/v1/outcome/", true), ("/v1/audit/", false)] {
        if let Some(id) = request.path().strip_prefix(prefix)
            && request.method() == Method::Get
        {
            let token = request.headers().get("authorization")?.unwrap_or_default();
            if token.is_empty() {
                return error(401, "unauthorized");
            }
            let tls_peer = identity::tls_peer(request);
            let proof = request.headers().get("dpop")?;
            let target = request.url()?;
            return archive::read(
                id,
                &token,
                outcome,
                env,
                &identity::RequestAuth {
                    tls_peer: tls_peer.as_deref(),
                    method: "GET",
                    authorization: &token,
                    proof: proof.as_deref(),
                    target: target.as_str(),
                },
            );
        }
    }
    if request.path() == "/healthz" {
        return Response::from_json(
            &serde_json::json!({"status":"ok","default_engine":"Statistical"}),
        );
    }
    if request.path() != "/v1/guard" || request.method() != Method::Post {
        return error(404, "not_found");
    }
    let body_start = js_sys::Date::now();
    let local_clock = env
        .var("AGINDOW_LATENCY_CLOCK")
        .is_ok_and(|value| value.to_string() == "workerd-local");
    let mut bytes = Vec::new();
    let mut stream = request.stream()?;
    let mut incoming_overhead = (js_sys::Date::now() - body_start).max(0.0);
    while let Some(chunk) = stream.next().await {
        let active = js_sys::Date::now();
        let chunk = chunk?;
        if bytes.len().saturating_add(chunk.len()) > CAP + 65536 {
            return error(413, "invalid_request");
        }
        bytes.extend_from_slice(&chunk);
        incoming_overhead += (js_sys::Date::now() - active).max(0.0);
    }
    let start = js_sys::Date::now();
    let input: Envelope = match serde_json::from_slice(&bytes) {
        Ok(input) => input,
        Err(_) => return error(400, "invalid_request"),
    };
    if input.validate().is_err() || input.connection.contains('/') || input.tool.contains('/') {
        return error(400, "invalid_request");
    }
    let token = request.headers().get("authorization")?.unwrap_or_default();
    let proof = request.headers().get("dpop")?;
    let target = request.url()?;
    let tls_peer = identity::tls_peer(request);
    let prepared = match state::prepare(
        env,
        &input,
        &token,
        proof.as_deref(),
        tls_peer.as_deref(),
        target.as_str(),
    ) {
        Ok(prepared) => prepared,
        Err(status) => {
            let reason = identity::denial();
            let response = error(
                status,
                reason.as_deref().unwrap_or(match status {
                    401 => "unauthorized",
                    403 => "plan_limit",
                    429 => "rate_limited",
                    400 => "invalid_request",
                    _ => "internal",
                }),
            )?;
            if status == 429 {
                response.headers().set("x-ratelimit-remaining", "0")?;
                response.headers().set("x-agindow-quota-scope", "isolate")?;
            }
            return Ok(response);
        }
    };
    let state::Prepared {
        route,
        permit,
        call,
    } = prepared;
    if call.as_ref().is_some_and(Execution::request_halted) {
        let completed = call
            .ok_or_else(state::internal)?
            .finish(&[], false)
            .map_err(|_| state::internal())?;
        archive::retain(
            &completed,
            &identity::Scope {
                tenant: &permit.tenant,
                environment: &route.environment,
                connection: &input.connection,
                tool: &input.tool,
            },
            &token,
            409,
            &Headers::new(),
            &[],
        )?;
        return policy(
            &completed,
            &permit,
            incoming_overhead + (js_sys::Date::now() - start).max(0.0),
            local_clock,
        );
    }
    let headers = Headers::new();
    headers.set("content-type", "application/json")?;
    for (name, value) in &route.headers {
        if ["host", "content-length", "connection", "transfer-encoding"]
            .contains(&name.to_ascii_lowercase().as_str())
        {
            return error(503, "internal");
        }
        headers.set(name, value)?;
    }
    if let Some(key) = &input.idempotency_key {
        headers.set("idempotency-key", key)?;
    }
    let method = match route.method.as_str() {
        "GET" => Method::Get,
        "HEAD" => Method::Head,
        "POST" => Method::Post,
        "PUT" => Method::Put,
        "PATCH" => Method::Patch,
        "DELETE" => Method::Delete,
        "OPTIONS" => Method::Options,
        _ => return error(503, "internal"),
    };
    let carries_body = !matches!(method, Method::Get | Method::Head);
    let mut init = RequestInit::new();
    init.with_method(method)
        .with_headers(headers)
        .with_redirect(worker::RequestRedirect::Manual);
    if carries_body {
        init.with_body(Some(js_sys::JsString::from(input.args.get()).into()));
    }
    let upstream_request = Request::new_with_init(&route.url, &init)?;
    let mut overhead = incoming_overhead + (js_sys::Date::now() - start).max(0.0);
    let mut upstream = Fetch::Request(upstream_request).send().await?;
    let status = upstream.status_code();
    let headers = Headers::from_iter(upstream.headers().entries());
    let identity_encoding = upstream
        .headers()
        .get("content-encoding")?
        .is_none_or(|value| value.eq_ignore_ascii_case("identity"));
    let inspect = identity_encoding
        && !route.selection.is_null()
        && upstream
            .headers()
            .get("content-length")?
            .and_then(|n| n.parse::<usize>().ok())
            .is_some_and(|n| n <= CAP);
    if !inspect {
        let active = js_sys::Date::now();
        let completed = call.and_then(|call| call.finish(&[], !route.selection.is_null()).ok());
        stamp(
            &headers,
            completed.as_ref(),
            &permit,
            overhead + (js_sys::Date::now() - active).max(0.0),
            local_clock,
        )?;
        return Ok(upstream.with_headers(headers));
    }
    let mut stream = upstream.stream()?;
    let mut body = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        let active = js_sys::Date::now();
        if body.len().saturating_add(chunk.len()) > CAP {
            let completed = call.and_then(|call| call.finish(&[], true).ok());
            stamp(
                &headers,
                completed.as_ref(),
                &permit,
                overhead + (js_sys::Date::now() - active).max(0.0),
                local_clock,
            )?;
            return Ok(Response::from_stream(
                stream::iter([Ok::<_, worker::Error>(body), Ok(chunk)]).chain(stream),
            )?
            .with_status(status)
            .with_headers(headers));
        }
        body.extend_from_slice(&chunk);
        overhead += (js_sys::Date::now() - active).max(0.0);
    }
    let active = js_sys::Date::now();
    let completed = call.and_then(|call| call.finish(&body, false).ok());
    overhead += (js_sys::Date::now() - active).max(0.0);
    if let Some(completed) = completed.as_ref()
        && completed.halted
    {
        if archive::retain(
            completed,
            &identity::Scope {
                tenant: &permit.tenant,
                environment: &route.environment,
                connection: &input.connection,
                tool: &input.tool,
            },
            &token,
            status,
            &headers,
            &body,
        )
        .is_err()
        {
            stamp(&headers, None, &permit, overhead, local_clock)?;
            return Ok(Response::from_bytes(body)?
                .with_status(status)
                .with_headers(headers));
        }
        let response = policy(completed, &permit, overhead, local_clock)?;
        response.headers().set(
            "x-agindow-outcome",
            &format!("/v1/outcome/{}", completed.artifact.id.as_str()),
        )?;
        return Ok(response);
    }
    stamp(&headers, completed.as_ref(), &permit, overhead, local_clock)?;
    Ok(Response::from_bytes(body)?
        .with_status(status)
        .with_headers(headers))
}
fn stamp(
    headers: &Headers,
    completed: Option<&Completed>,
    permit: &Permit,
    overhead: f64,
    local_clock: bool,
) -> Result<()> {
    let owned: Vec<_> = headers
        .keys()
        .filter(|name| name.starts_with("x-agindow-"))
        .collect();
    for name in owned {
        headers.delete(&name)?;
    }
    let status = match completed {
        None => "bypassed",
        Some(c) if c.bypassed => "bypassed",
        Some(c) if c.halted => "blocked",
        Some(c) => match c.artifact.decision {
            agw_audit::contracts::audit_artifact::AuditArtifactDecision::Degraded => "degraded",
            _ => "ok",
        },
    };
    headers.set("x-agindow-status", status)?;
    if local_clock {
        headers.set("x-agindow-latency-ms", &format!("{:.0}", overhead.floor()))?;
        headers.set("x-agindow-latency-clock", "workerd-local")?;
    } else {
        headers.set("x-agindow-latency-ms", "unavailable")?;
        headers.set("x-agindow-latency-clock", "unavailable")?;
    }
    headers.set("x-ratelimit-limit", &permit.limit.to_string())?;
    headers.set("x-ratelimit-remaining", &permit.remaining.to_string())?;
    headers.set("x-ratelimit-reset", &permit.resets.to_string())?;
    headers.set("x-agindow-quota-scope", "isolate")?;
    if let Some(completed) = completed {
        headers.set("x-agindow-mode", &completed.artifact.mode.to_string())?;
        headers.set("x-agindow-audit", completed.artifact.id.as_str())?;
        if let Some(drift) = &completed.artifact.drift {
            headers.set("x-agindow-drift-class", &drift.class.to_string())?;
            headers.set("x-agindow-drift-side", &drift.side.to_string())?;
        }
    }
    Ok(())
}
fn policy(
    completed: &Completed,
    permit: &Permit,
    overhead: f64,
    local_clock: bool,
) -> Result<Response> {
    let feedback = completed.feedback.as_ref().ok_or_else(state::internal)?;
    let response = Response::from_json(feedback)?.with_status(409);
    stamp(
        response.headers(),
        Some(completed),
        permit,
        overhead,
        local_clock,
    )?;
    Ok(response)
}
