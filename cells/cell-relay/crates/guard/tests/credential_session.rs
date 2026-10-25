use agw_guard::{
    credential::session::{Context, Session},
    proxy::{Headers, UpstreamResponse},
};
use std::io::{Cursor, Read};
#[test]
fn response_bytes_and_status_survive_additive_evidence() -> Result<(), Box<dyn std::error::Error>> {
    let mut session = Session::new(16, 3, 1000)?;
    let mut headers = Headers::new();
    headers.push("Authorization", "sk_live_AGINDOWcanary123");
    let mut c = Context {
        id: "event-1",
        rotation_id: "rotation-1",
        ts: "2026-10-22T00:00:00Z",
        tenant: "tenant-1",
        connection: "conn-1",
        operation: "read",
        arguments: "args-1",
        version: Some("v1"),
        audit_ref: Some("audit-1"),
        now: 0,
        available: true,
        complete: true,
        revoked: false,
    };
    let body = b"{\"error\":\"bad key sk_live_AGINDOWcanary123\"}";
    let forwarded = session.forward(
        &c,
        UpstreamResponse {
            status: 401,
            headers: headers.clone(),
            body: Cursor::new(body),
        },
        std::time::Duration::ZERO,
        Some(body),
    );
    assert_eq!(forwarded.response.status, 401);
    let mut received = Vec::new();
    let mut response = forwarded.response;
    response.body.read_to_end(&mut received)?;
    assert_eq!(received, body);
    assert!(forwarded.events[1].is_some());
    c.id = "event-2";
    assert!(session.observe(&c, 200, &headers, None)[1].is_some());
    c.id = "event-3";
    assert!(session.observe(&c, 200, &headers, None)[1].is_none());
    c.version = Some("v2");
    c.id = "event-4";
    let events = session.observe(&c, 401, &headers, None);
    assert!(events.iter().all(Option::is_some));
    assert!(!serde_json::to_string(&events)?.contains("sk_live_AGINDOWcanary123"));
    c.available = false;
    assert!(
        session
            .observe(&c, 401, &headers, None)
            .iter()
            .all(Option::is_none)
    );
    Ok(())
}

#[test]
fn the_entire_error_taxonomy_is_forwarded_without_changing_codes_or_retryability()
-> Result<(), Box<dyn std::error::Error>> {
    for (status, code, retryable, credential) in [
        (400, "invalid_request", false, false),
        (401, "unauthorized", false, true),
        (403, "plan_limit", false, false),
        (409, "drift_blocked", false, false),
        (409, "drift_halted", false, false),
        (422, "contract_violation", false, false),
        (429, "rate_limited", true, false),
        (500, "internal", true, false),
        (503, "internal", true, false),
    ] {
        let mut session = Session::new(16, 3, 1000)?;
        let c = Context {
            id: "event",
            rotation_id: "rotation",
            ts: "2026-10-22T00:00:00Z",
            tenant: "tenant",
            connection: "connection",
            operation: "read",
            arguments: "args",
            version: Some("v1"),
            audit_ref: None,
            now: 0,
            available: true,
            complete: true,
            revoked: false,
        };
        let body = serde_json::to_vec(&serde_json::json!({
            "error": code, "retryable": retryable, "detail": "sk_live_AGINDOWcanary123",
        }))?;
        let forwarded = session.forward(
            &c,
            UpstreamResponse {
                status,
                headers: Headers::new(),
                body: Cursor::new(body.as_slice()),
            },
            std::time::Duration::ZERO,
            Some(&body),
        );
        assert_eq!(forwarded.response.status, status);
        let mut response = forwarded.response.body.into_inner();
        assert_eq!(response.get_ref().as_ptr(), body.as_ptr());
        let mut received = Vec::new();
        response.read_to_end(&mut received)?;
        assert_eq!(received, body);
        assert_eq!(forwarded.events[1].is_some(), credential);
        assert!(!serde_json::to_string(&forwarded.events)?.contains("sk_live_AGINDOWcanary123"));
    }
    Ok(())
}

#[test]
fn unavailable_or_incomplete_observation_never_changes_the_original_response()
-> Result<(), Box<dyn std::error::Error>> {
    let oversized = vec![b' '; 4097];
    for (available, complete, body) in [
        (false, true, b"{}".as_slice()),
        (true, false, b"{\"cut".as_slice()),
        (true, true, b"not JSON".as_slice()),
        (true, true, oversized.as_slice()),
    ] {
        let mut session = Session::new(16, 3, 1000)?;
        let c = Context {
            id: "event",
            rotation_id: "rotation",
            ts: "2026-10-22T00:00:00Z",
            tenant: "tenant",
            connection: "connection",
            operation: "read",
            arguments: "args",
            version: None,
            audit_ref: None,
            now: 0,
            available,
            complete,
            revoked: false,
        };
        let forwarded = session.forward(
            &c,
            UpstreamResponse {
                status: 401,
                headers: Headers::new(),
                body: Cursor::new(body),
            },
            std::time::Duration::ZERO,
            Some(body),
        );
        assert_eq!(forwarded.response.status, 401);
        assert_eq!(
            forwarded.response.body.into_inner().get_ref().as_ptr(),
            body.as_ptr()
        );
        assert!(forwarded.events.iter().all(Option::is_none));
    }
    Ok(())
}
