use agw_guard::credential::{
    Cause, Evidence,
    health::{Health, Projection, Update},
};
use std::sync::{Arc, Mutex};

#[test]
fn ten_thousand_failure_recovery_flaps_keep_one_record_and_one_transition_per_recovery()
-> Result<(), Box<dyn std::error::Error>> {
    let mut p = Projection::new("t", 10, 3, 100)?;
    let mut recoveries = 0;
    for n in 0..10000 {
        let sequence = n * 3 + 1;
        p.apply(&Update {
            tenant: "t",
            connection: "c",
            version: Some("v1"),
            prior_version: None,
            cause: Cause::Rejected,
            evidence: Evidence::Unknown,
            now: n,
            sequence,
        });
        recoveries += usize::from(p.success("t", "c", Some("v1"), n, sequence + 1));
        assert!(!p.success("t", "c", Some("v1"), n, sequence + 2));
        assert_eq!(p.len(), 1);
    }
    assert_eq!(recoveries, 10000);
    assert_eq!(p.dropped(), 0);
    assert_eq!(
        p.get("c", Some("v1")).map(|r| (r.health, r.revision)),
        Some((Health::Healthy, 30000))
    );
    Ok(())
}

#[test]
fn concurrent_host_projection_updates_emit_one_recovery_with_monotonic_revisions()
-> Result<(), Box<dyn std::error::Error>> {
    let mut p = Projection::new("t", 10, 3, 100)?;
    p.apply(&Update {
        tenant: "t",
        connection: "c",
        version: Some("v1"),
        prior_version: None,
        cause: Cause::Rejected,
        evidence: Evidence::Unknown,
        now: 1,
        sequence: 1,
    });
    // The host serializes projection admission after dispatch; the library
    // never takes this host lock or places it on the upstream call path.
    let state = Arc::new(Mutex::new(p));
    let mut workers = Vec::new();
    for _ in 0..32 {
        let state = Arc::clone(&state);
        workers.push(std::thread::spawn(move || -> Result<bool, &'static str> {
            let mut p = state.lock().map_err(|_| "projection poisoned")?;
            let revision = p.get("c", Some("v1")).ok_or("missing record")?.revision;
            let recovered = p.success("t", "c", Some("v1"), 2, revision + 1);
            assert_eq!(
                p.get("c", Some("v1")).map(|r| r.revision),
                Some(revision + 1)
            );
            Ok(recovered)
        }));
    }
    let mut recoveries = 0;
    for worker in workers {
        recoveries += usize::from(worker.join().map_err(|_| "worker panicked")??);
    }
    assert_eq!(recoveries, 1);
    assert_eq!(
        state
            .lock()
            .map_err(|_| "projection poisoned")?
            .get("c", Some("v1"))
            .map(|r| r.revision),
        Some(33)
    );
    Ok(())
}

#[test]
fn superseded_versions_expire_by_host_time_without_losing_the_current_version()
-> Result<(), Box<dyn std::error::Error>> {
    let mut p = Projection::new("t", 4, 3, 100)?;
    p.success("t", "c", Some("v1"), 1, 1);
    p.apply(&Update {
        tenant: "t",
        connection: "c",
        version: Some("v2"),
        prior_version: Some("v1"),
        cause: Cause::Rotated,
        evidence: Evidence::HostDeclared,
        now: 2,
        sequence: 1,
    });
    p.success("t", "other", None, 50, 1);
    assert_eq!(
        p.get("c", Some("v1")).map(|r| r.health),
        Some(Health::Superseded)
    );
    p.success("t", "later", None, 102, 1);
    assert!(p.get("c", Some("v1")).is_none());
    assert_eq!(
        p.get("c", Some("v2")).map(|r| r.health),
        Some(Health::Degrading)
    );
    assert_eq!(p.dropped(), 0);
    Ok(())
}
