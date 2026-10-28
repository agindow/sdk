use super::State;
use crate::{
    Envelope,
    execution::{Completed, Execution},
};
use axum::{
    body::{Body, Bytes},
    extract::State as Extract,
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use futures_util::{StreamExt, stream};
use serde_json::json;
use std::{
    sync::{Arc, atomic::Ordering},
    time::{Duration, Instant},
};
const CAP: usize = 8 * 1024 * 1024;

pub async fn health(Extract(state): Extract<Arc<State>>) -> Response {
    axum::Json(json!({ "status": "ok", "instance": state.settings.instance,
        "registered": true, "calls": state.calls.load(Ordering::Relaxed) }))
    .into_response()
}
pub async fn registration(Extract(state): Extract<Arc<State>>) -> Response {
    axum::Json(json!({ "instance": state.settings.instance,
        "routes": state.settings.routes.keys().collect::<Vec<_>>(), "default_engine": "Statistical" })).into_response()
}
fn error(status: StatusCode, code: &'static str) -> Response {
    (status, axum::Json(json!({ "error": code }))).into_response()
}
pub async fn guard(Extract(state): Extract<Arc<State>>, bytes: Bytes) -> Response {
    let started = Instant::now();
    let input: Envelope = match serde_json::from_slice(&bytes) {
        Ok(input) => input,
        Err(_) => return error(StatusCode::BAD_REQUEST, "invalid_request"),
    };
    if input.validate().is_err() || input.tool.contains('/') || input.connection.contains('/') {
        return error(StatusCode::BAD_REQUEST, "invalid_request");
    }
    let key = format!("{}/{}", input.connection, input.tool);
    let Some(route) = state.settings.routes.get(&key) else {
        return error(StatusCode::UNAUTHORIZED, "unauthorized");
    };
    if input.operation != route.operation {
        return error(StatusCode::BAD_REQUEST, "invalid_request");
    }
    let Some(engine) = state.engines.get(&key) else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "internal");
    };
    let sequence = state.calls.fetch_add(1, Ordering::Relaxed);
    let id = format!(
        "aud_{}_{}_{}",
        state.settings.instance,
        chrono::Utc::now().timestamp_micros(),
        sequence
    );
    let call = Execution::start(&input, engine, id, state.settings.work).ok();
    if call.as_ref().is_some_and(Execution::request_halted) {
        return match call.and_then(|call| call.finish(&[], true).ok()) {
            Some(completed) => {
                if let Ok(mut archive) = state.archive.try_lock() {
                    archive.insert(super::archive::Entry {
                        id: completed.artifact.id.to_string(),
                        artifact: completed.artifact.clone(),
                        original: None,
                    });
                }
                policy(completed, input.mode, started.elapsed())
            }
            None => error(StatusCode::SERVICE_UNAVAILABLE, "internal"),
        };
    }
    let method = match reqwest::Method::from_bytes(route.method.as_bytes()) {
        Ok(method) => method,
        Err(_) => return error(StatusCode::SERVICE_UNAVAILABLE, "internal"),
    };
    let mut request = state
        .client
        .request(method, &route.url)
        .body(input.args.get().to_owned())
        .header("content-type", "application/json");
    for (name, value) in &route.headers {
        request = request.header(name, value);
    }
    if let Some(key) = &input.idempotency_key {
        request = request.header("idempotency-key", key);
    }
    let mut overhead = started.elapsed();
    let mut upstream = match request.send().await {
        Ok(upstream) => upstream,
        Err(_) => return error(StatusCode::BAD_GATEWAY, "internal"),
    };
    let status = upstream.status();
    let mut headers = upstream.headers().clone();
    // No engine, faulted engine and streaming captures retain the HTTP stream.
    let identity_encoding = upstream
        .headers()
        .get("content-encoding")
        .is_none_or(|value| value == "identity");
    let inspect = identity_encoding
        && !route.selection.is_null()
        && upstream.content_length().is_some_and(|n| n <= CAP as u64);
    if !inspect {
        let active = Instant::now();
        let completed = call.and_then(|call| call.finish(&[], !route.selection.is_null()).ok());
        stamp(
            &mut headers,
            input.mode,
            completed.as_ref(),
            overhead + active.elapsed(),
        );
        return response(status, headers, Body::from_stream(upstream.bytes_stream()));
    }
    let mut parts = Vec::new();
    let mut length = 0;
    loop {
        let chunk = match upstream.chunk().await {
            Ok(chunk) => chunk,
            Err(_) => return error(StatusCode::BAD_GATEWAY, "internal"),
        };
        let active = Instant::now();
        let Some(chunk) = chunk else {
            overhead += active.elapsed();
            break;
        };
        length += chunk.len();
        parts.push(chunk);
        overhead += active.elapsed();
        if length > CAP {
            let active = Instant::now();
            let completed = call.and_then(|call| call.finish(&[], true).ok());
            stamp(
                &mut headers,
                input.mode,
                completed.as_ref(),
                overhead + active.elapsed(),
            );
            let prefix = stream::iter(parts.into_iter().map(Ok::<_, reqwest::Error>));
            return response(
                status,
                headers,
                Body::from_stream(prefix.chain(upstream.bytes_stream())),
            );
        }
    }
    let active = Instant::now();
    let mut body = Vec::with_capacity(length);
    for part in parts {
        body.extend_from_slice(&part);
    }
    let completed = call.and_then(|call| call.finish(&body, false).ok());
    overhead += active.elapsed();
    let retained = if let Some(completed) = completed.as_ref()
        && let Ok(mut archive) = state.archive.try_lock()
    {
        archive.insert(super::archive::Entry {
            id: completed.artifact.id.to_string(),
            artifact: completed.artifact.clone(),
            original: completed
                .halted
                .then(|| (status, headers.clone(), Bytes::copy_from_slice(&body))),
        })
    } else {
        false
    };
    if completed.as_ref().is_some_and(|done| done.halted) && !retained {
        stamp(&mut headers, input.mode, None, overhead + active.elapsed());
        return response(status, headers, Body::from(body));
    }
    if let Some(completed) = completed.as_ref()
        && completed.halted
    {
        // The original completed HTTP outcome is retained separately for the
        // host's recovery surface; the feedback forbids blind write replay.
        let mut result = policy_ref(completed, input.mode, overhead);
        if let Ok(value) = HeaderValue::from_str(&status.as_u16().to_string()) {
            result
                .headers_mut()
                .insert("x-agindow-upstream-status", value);
        }
        if let Ok(location) =
            HeaderValue::from_str(&format!("/v1/outcome/{}", completed.artifact.id.as_str()))
        {
            result.headers_mut().insert("x-agindow-outcome", location);
        }
        return result;
    }
    stamp(&mut headers, input.mode, completed.as_ref(), overhead);
    response(status, headers, Body::from(body))
}
fn response(status: StatusCode, headers: HeaderMap, body: Body) -> Response {
    let mut response = Response::new(body);
    *response.status_mut() = status;
    *response.headers_mut() = headers;
    response
}
fn stamp(
    headers: &mut HeaderMap,
    mode: agw_guard::config::Mode,
    completed: Option<&Completed>,
    overhead: Duration,
) {
    let owned: Vec<_> = headers
        .keys()
        .filter(|name| name.as_str().starts_with("x-agindow-"))
        .cloned()
        .collect();
    for name in owned {
        headers.remove(name);
    }
    let status = match completed {
        None => "bypassed",
        Some(result) if result.bypassed => "bypassed",
        Some(result) if result.halted => "blocked",
        Some(result) => match result.artifact.decision {
            agw_audit::contracts::audit_artifact::AuditArtifactDecision::Degraded => "degraded",
            _ => "ok",
        },
    };
    headers.insert("x-agindow-status", HeaderValue::from_static(status));
    if let Ok(mode) = HeaderValue::from_str(&mode.to_string()) {
        headers.insert("x-agindow-mode", mode);
    }
    if let Ok(latency) = HeaderValue::from_str(&overhead.as_millis().to_string()) {
        headers.insert("x-agindow-latency-ms", latency);
    }
    if let Some(result) = completed {
        if let Ok(id) = HeaderValue::from_str(&result.artifact.id) {
            headers.insert("x-agindow-audit", id);
        }
        if let Some(drift) = &result.artifact.drift {
            if let Ok(class) = HeaderValue::from_str(&drift.class.to_string()) {
                headers.insert("x-agindow-drift-class", class);
            }
            if let Ok(side) = HeaderValue::from_str(&drift.side.to_string()) {
                headers.insert("x-agindow-drift-side", side);
            }
        }
    }
}
fn policy(completed: Completed, mode: agw_guard::config::Mode, overhead: Duration) -> Response {
    policy_ref(&completed, mode, overhead)
}
fn policy_ref(
    completed: &Completed,
    mode: agw_guard::config::Mode,
    overhead: Duration,
) -> Response {
    let Some(feedback) = &completed.feedback else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "internal");
    };
    let mut result = (StatusCode::CONFLICT, axum::Json(feedback)).into_response();
    stamp(result.headers_mut(), mode, Some(completed), overhead);
    result
}

pub async fn audit(
    Extract(state): Extract<Arc<State>>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Response {
    let Ok(archive) = state.archive.lock() else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "internal");
    };
    match archive.get(&id) {
        Some(entry) => axum::Json(&entry.artifact).into_response(),
        None => error(StatusCode::NOT_FOUND, "not_found"),
    }
}
pub async fn outcome(
    Extract(state): Extract<Arc<State>>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Response {
    let Ok(archive) = state.archive.lock() else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "internal");
    };
    match archive.get(&id).and_then(|entry| entry.original.as_ref()) {
        Some((status, headers, bytes)) => {
            let mut retained = headers.clone();
            retained.insert(
                "x-agindow-retained-outcome",
                HeaderValue::from_static("true"),
            );
            response(*status, retained, Body::from(bytes.clone()))
        }
        None => error(StatusCode::NOT_FOUND, "not_found"),
    }
}
