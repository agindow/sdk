use agw_guard::credential::{Cause, Evidence, Signal, classify};
use agw_guard::proxy::Headers;
#[test]
fn bounded_evidence_preserves_taxonomy() {
    let mut headers = Headers::new();
    headers.push("Authorization", "sk_live_AGINDOWcanary123");
    for (status, expected) in [
        (200, None),
        (400, None),
        (401, Some((Cause::Rejected, Evidence::Unknown))),
        (403, None),
        (429, None),
        (500, None),
    ] {
        let signal = Signal {
            status,
            headers: &headers,
            body: Some(b"{}"),
            complete: true,
            available: true,
            revoked: false,
        };
        assert_eq!(classify(&signal), expected);
    }
    for (directive, expected) in [
        ("Bearer error=\"invalid_token\"", Cause::Rejected),
        ("Bearer error=\"expired_token\"", Cause::Expired),
    ] {
        let mut h = headers.clone();
        h.push("WWW-Authenticate", directive);
        assert_eq!(
            classify(&Signal {
                status: 401,
                headers: &h,
                body: None,
                complete: true,
                available: true,
                revoked: false
            }),
            Some((expected, Evidence::ProviderDeclared))
        );
    }
}
#[test]
fn quota_and_faults_are_not_dead_credentials() {
    for (name, value) in [("Retry-After", "1"), ("X-RateLimit-Remaining", "0")] {
        let mut h = Headers::new();
        h.push(name, value);
        let mut signal = Signal {
            status: 403,
            headers: &h,
            body: Some(b"{}"),
            complete: true,
            available: true,
            revoked: false,
        };
        assert_eq!(
            classify(&signal),
            Some((Cause::Quota, Evidence::ProviderDeclared))
        );
        signal.available = false;
        assert_eq!(classify(&signal), None);
        signal.available = true;
        signal.complete = false;
        assert_eq!(classify(&signal), None);
    }
}

#[test]
fn scope_and_recovery_are_version_and_argument_scoped() -> Result<(), Box<dyn std::error::Error>> {
    use agw_guard::credential::tracker::{Observation, Tracker};
    let mut tracker = Tracker::new(2, 3, 100)?;
    let h = Headers::new();
    let mut o = Observation {
        operation: "read",
        arguments: "args-1",
        version: Some("v1"),
        now: 0,
        signal: Signal {
            status: 200,
            headers: &h,
            body: None,
            complete: true,
            available: true,
            revoked: false,
        },
    };
    assert_eq!(tracker.observe(&o), None);
    o.signal.status = 403;
    for now in 1..3 {
        o.now = now;
        assert_eq!(tracker.observe(&o), None);
    }
    o.now = 3;
    assert_eq!(
        tracker.observe(&o),
        Some((Cause::ScopeReduced, Evidence::Inferred))
    );
    o.signal.status = 200;
    assert_eq!(
        tracker.observe(&o),
        Some((Cause::Recovered, Evidence::ProviderDeclared))
    );
    assert_eq!(tracker.observe(&o), None);
    o.version = Some("v2");
    assert_eq!(
        tracker.observe(&o),
        Some((Cause::Rotated, Evidence::HostDeclared))
    );
    o.signal.status = 403;
    o.arguments = "args-2";
    assert_eq!(tracker.observe(&o), None);
    Ok(())
}
#[test]
fn health_is_bounded_tenant_scoped_and_hysteretic() -> Result<(), Box<dyn std::error::Error>> {
    use agw_guard::credential::health::{Health, Projection, Update};
    let mut p = Projection::new("tenant-1", 2, 3, 100)?;
    let mut u = Update {
        tenant: "tenant-1",
        connection: "conn-1",
        version: Some("v1"),
        prior_version: None,
        cause: Cause::Rejected,
        evidence: Evidence::Unknown,
        now: 0,
        sequence: 1,
    };
    for n in 1..=3 {
        u.sequence = n;
        u.now = n;
        p.apply(&u);
        assert_eq!(
            p.get("conn-1", Some("v1")).map(|r| r.health),
            Some(if n < 3 {
                Health::Degrading
            } else {
                Health::Failing
            })
        );
    }
    assert_eq!(
        p.get("conn-1", Some("v1")).map(|r| r.health),
        Some(Health::Failing)
    );
    u.cause = Cause::Recovered;
    u.sequence = 4;
    assert!(p.apply(&u));
    assert_eq!(
        p.get("conn-1", Some("v1")).map(|r| r.health),
        Some(Health::Healthy)
    );
    assert!(!p.apply(&u));
    u.sequence = 5;
    assert!(!p.apply(&u));
    u.sequence = 6;
    u.tenant = "other";
    assert!(!p.apply(&u));
    u.tenant = "tenant-1";
    for c in ["conn-2", "conn-3"] {
        u.connection = c;
        p.apply(&u);
    }
    assert_eq!(p.len(), 2);
    assert_eq!(p.dropped(), 1);
    assert!(p.get("conn-1", Some("v1")).is_none());
    Ok(())
}
#[test]
fn capacity_and_expiry_stay_bounded_under_ten_times_traffic()
-> Result<(), Box<dyn std::error::Error>> {
    use agw_guard::credential::health::{Health, Projection, Update};
    let mut p = Projection::new("t", 10, 3, 100)?;
    for n in 0..100 {
        let connection = format!("c{n}");
        p.apply(&Update {
            tenant: "t",
            connection: &connection,
            version: Some("v1"),
            prior_version: None,
            cause: Cause::Expired,
            evidence: Evidence::ProviderDeclared,
            now: n,
            sequence: 1,
        });
        assert!(p.len() <= 10);
    }
    assert_eq!(p.dropped(), 90);
    assert_eq!(
        p.get("c99", Some("v1")).map(|r| r.health),
        Some(Health::Failing)
    );
    assert!(p.expiry("c99", Some("v1"), 200, 150, 50));
    assert!(!p.expiry("c99", Some("v1"), 200, 160, 50));
    Ok(())
}
proptest::proptest! {
    #[test]
    fn arbitrary_directives_never_panic(value in proptest::collection::vec(proptest::num::u8::ANY, 0..8192)) {
        let mut headers = Headers::new(); headers.push("WWW-Authenticate",String::from_utf8_lossy(&value));
        let _ = classify(&Signal { status:401, headers:&headers, body:None, complete:true, available:true, revoked:false });
    }
}
#[test]
fn bounded_timestamp_window_converges_in_canonical_order() -> Result<(), Box<dyn std::error::Error>>
{
    use agw_guard::credential::health::{Projection, Update};
    let failure = Update {
        tenant: "t",
        connection: "c",
        version: Some("v1"),
        prior_version: None,
        cause: Cause::Rejected,
        evidence: Evidence::Unknown,
        now: 1,
        sequence: 1,
    };
    let mut left = [
        failure,
        Update {
            sequence: 2,
            ..failure
        },
        Update {
            sequence: 3,
            ..failure
        },
    ];
    let mut right = [left[2], left[0], left[1]];
    let mut p = Projection::new("t", 10, 3, 100)?;
    let mut q = Projection::new("t", 10, 3, 100)?;
    assert!(p.apply_window(&mut left));
    assert!(q.apply_window(&mut right));
    assert_eq!(p.get("c", Some("v1")), q.get("c", Some("v1")));
    Ok(())
}
#[test]
fn raw_success_and_rotation_keep_health_and_cause_separate()
-> Result<(), Box<dyn std::error::Error>> {
    use agw_guard::credential::health::{Health, Projection, Update};
    let mut p = Projection::new("t", 10, 3, 100)?;
    assert!(!p.success("t", "c", Some("v1"), 1, 1));
    assert_eq!(
        p.get("c", Some("v1")).map(|r| r.health),
        Some(Health::Healthy)
    );
    p.apply(&Update {
        tenant: "t",
        connection: "c",
        version: Some("v2"),
        prior_version: Some("v1"),
        cause: Cause::Rotated,
        evidence: Evidence::HostDeclared,
        now: 2,
        sequence: 2,
    });
    assert_eq!(
        p.get("c", Some("v1")).map(|r| r.health),
        Some(Health::Superseded)
    );
    assert_eq!(
        p.get("c", Some("v2")).map(|r| r.health),
        Some(Health::Degrading)
    );
    assert!(!p.success("t", "c", Some("v2"), 3, 3));
    assert_eq!(
        p.get("c", Some("v2")).map(|r| r.last_cause),
        Some(Cause::Rotated)
    );
    Ok(())
}
#[test]
fn malformed_or_quoted_description_never_claims_expiry_authority() {
    for value in [
        "Bearer error=\"expired_token",
        "Bearer error_description=\"error=expired_token\"",
        "Bearer error_description=\"ignored, error=expired_token\"",
    ] {
        let mut headers = Headers::new();
        headers.push("WWW-Authenticate", value);
        assert_eq!(
            classify(&Signal {
                status: 401,
                headers: &headers,
                body: None,
                complete: true,
                available: true,
                revoked: false
            }),
            Some((Cause::Rejected, Evidence::Unknown))
        );
    }
    let mut headers = Headers::new();
    headers.push("WWW-Authenticate", "Bearer error = \"expired_token\"");
    assert_eq!(
        classify(&Signal {
            status: 401,
            headers: &headers,
            body: None,
            complete: true,
            available: true,
            revoked: false
        }),
        Some((Cause::Expired, Evidence::ProviderDeclared))
    );
}
#[test]
fn late_old_binding_outcome_does_not_unsupersede_it() -> Result<(), Box<dyn std::error::Error>> {
    use agw_guard::credential::health::{Health, Projection, Update};
    let mut p = Projection::new("t", 10, 3, 100)?;
    p.success("t", "c", Some("v1"), 0, 1);
    p.apply(&Update {
        tenant: "t",
        connection: "c",
        version: Some("v2"),
        prior_version: Some("v1"),
        cause: Cause::Rotated,
        evidence: Evidence::HostDeclared,
        now: 1,
        sequence: 1,
    });
    p.apply(&Update {
        tenant: "t",
        connection: "c",
        version: Some("v1"),
        prior_version: None,
        cause: Cause::Rejected,
        evidence: Evidence::Unknown,
        now: 2,
        sequence: 2,
    });
    assert_eq!(
        p.get("c", Some("v1")).map(|r| r.health),
        Some(Health::Superseded)
    );
    Ok(())
}

#[test]
fn host_revocation_and_explicit_quota_extensions_preserve_evidence_authority() {
    use agw_guard::credential::classify_with_phrases;
    let headers = Headers::new();
    let mut signal = Signal {
        status: 401,
        headers: &headers,
        body: None,
        complete: true,
        available: true,
        revoked: true,
    };
    assert_eq!(
        classify(&signal),
        Some((Cause::Revoked, Evidence::HostDeclared))
    );
    signal.revoked = false;
    signal.status = 403;
    signal.body = Some(b"{\"error\":\"entitlement depleted\"}");
    assert_eq!(classify(&signal), None);
    assert_eq!(
        classify_with_phrases(&signal, &["entitlement depleted"]),
        Some((Cause::Quota, Evidence::ProviderDeclared))
    );
    signal.complete = false;
    assert_eq!(
        classify_with_phrases(&signal, &["entitlement depleted"]),
        None
    );
}
