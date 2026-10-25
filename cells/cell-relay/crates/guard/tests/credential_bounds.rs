use agw_guard::credential::{
    Cause, Evidence, Signal, classify,
    tracker::{Observation, Tracker},
};
use agw_guard::proxy::Headers;

#[test]
fn malformed_or_oversized_bodies_cannot_seed_scope_inference_or_consume_recovery()
-> Result<(), Box<dyn std::error::Error>> {
    let headers = Headers::new();
    let oversized = vec![b' '; 4097];
    for body in [b"not JSON".as_slice(), oversized.as_slice()] {
        let mut tracker = Tracker::new(10, 3, 100)?;
        let mut o = Observation {
            operation: "read",
            arguments: "args",
            version: Some("v1"),
            now: 0,
            signal: Signal {
                status: 200,
                headers: &headers,
                body: None,
                complete: true,
                available: true,
                revoked: false,
            },
        };
        assert_eq!(tracker.observe(&o), None);
        o.signal.status = 403;
        o.signal.body = Some(body);
        for now in 1..=3 {
            o.now = now;
            assert_eq!(tracker.observe(&o), None);
        }
        o.signal.body = None;
        for now in 4..6 {
            o.now = now;
            assert_eq!(tracker.observe(&o), None);
        }
        o.now = 6;
        assert_eq!(
            tracker.observe(&o),
            Some((Cause::ScopeReduced, Evidence::Inferred))
        );
        o.signal.status = 200;
        o.signal.body = Some(body);
        assert_eq!(tracker.observe(&o), None);
        o.signal.body = None;
        assert_eq!(
            tracker.observe(&o),
            Some((Cause::Recovered, Evidence::ProviderDeclared))
        );
        assert_eq!(tracker.observe(&o), None);
    }
    Ok(())
}

#[test]
fn oversized_auth_markers_and_header_collections_are_ineligible_evidence() {
    for (name, status) in [("X-RateLimit-Remaining", 403), ("WWW-Authenticate", 401)] {
        let mut headers = Headers::new();
        headers.push(name, format!("{}0", " ".repeat(4097)));
        assert_eq!(
            classify(&Signal {
                status,
                headers: &headers,
                body: None,
                complete: true,
                available: true,
                revoked: false
            }),
            None
        );
    }
    let mut headers = Headers::new();
    for _ in 0..129 {
        headers.push("X-Other", "ignored");
    }
    headers.push("WWW-Authenticate", "Bearer error=\"expired_token\"");
    assert_eq!(
        classify(&Signal {
            status: 401,
            headers: &headers,
            body: None,
            complete: true,
            available: true,
            revoked: false
        }),
        None
    );
}
