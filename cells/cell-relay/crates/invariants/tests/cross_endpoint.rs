use agw_invariants::cross_endpoint::{
    Endpoint, Key, Limits, Observation, Reason, Rule, Scope, Side, Store, compare,
};
fn endpoint(operation: &str) -> Endpoint {
    Endpoint {
        operation: operation.into(),
        side: Side::Response,
        field: "/amount".into(),
    }
}
fn key(operation: &str) -> Key {
    Key {
        endpoint: endpoint(operation),
        concept: "gross".into(),
        unit: "usd-cent".into(),
        contract_version: 1,
    }
}
fn scope() -> Scope {
    Scope {
        tenant: "t".into(),
        connection: "c".into(),
        vendor: "v".into(),
        workflow: "w".into(),
        entity: "e".into(),
        revision: "r1".into(),
    }
}
fn observation(operation: &str, value: Option<i64>, at_ms: i64, sequence: u64) -> Observation {
    Observation {
        scope: scope(),
        key: key(operation),
        value,
        at_ms,
        sequence,
    }
}
fn rule() -> Rule {
    Rule {
        id: "same-gross".into(),
        left: endpoint("charge"),
        right: endpoint("balance"),
        concept: "gross".into(),
        unit: "usd-cent".into(),
        tolerance: 1,
        max_age_ms: 300_000,
        max_skew_ms: 120_000,
    }
}
#[test]
fn paired_values_obey_numeric_age_and_skew_boundaries() -> Result<(), Box<dyn std::error::Error>> {
    let rule = rule();
    let a = observation("charge", Some(100), 0, 1);
    let b = observation("balance", Some(101), 120_000, 2);
    assert_eq!(compare(&rule, Some(&a), Some(&b), 300_000)?, Reason::Pass);
    let bad = Observation {
        value: Some(102),
        ..b.clone()
    };
    assert_eq!(
        compare(&rule, Some(&a), Some(&bad), 300_000)?,
        Reason::Divergence
    );
    assert_eq!(compare(&rule, Some(&a), Some(&b), 300_001)?, Reason::Stale);
    assert_eq!(compare(&rule, Some(&a), Some(&b), 119_999)?, Reason::Future);
    assert_eq!(
        compare(
            &rule,
            Some(&a),
            Some(&Observation {
                at_ms: 120_001,
                ..b.clone()
            }),
            120_001
        )?,
        Reason::Skew
    );
    assert_eq!(compare(&rule, Some(&a), None, 120_000)?, Reason::Partial);
    assert_eq!(
        compare(
            &rule,
            Some(&a),
            Some(&Observation {
                value: None,
                ..b.clone()
            }),
            120_000
        )?,
        Reason::Unsupported
    );
    let mut unrelated = b;
    unrelated.scope.entity = "other".into();
    assert_eq!(
        compare(&rule, Some(&a), Some(&unrelated), 120_000)?,
        Reason::ScopeMismatch
    );
    Ok(())
}
#[test]
fn store_publishes_atomically_and_never_replaces_newer_or_invalidated_evidence()
-> Result<(), Box<dyn std::error::Error>> {
    let store = Store::new(Limits {
        scopes: 1,
        observations_per_scope: 2,
    })?;
    store.record(&[observation("charge", Some(100), 0, 1)])?;
    let first = store.snapshot(&scope())?;
    store.record(&[observation("balance", Some(100), 1000, 2)])?;
    assert_eq!(first.observations().len(), 1);
    assert!(
        store
            .record(&[observation("charge", Some(999), -1, 3)])
            .is_err()
    );
    assert!(
        store
            .record(&[observation("charge", Some(999), 2000, 1)])
            .is_err()
    );
    assert!(
        store
            .record(&[observation("third", Some(1), 2000, 3)])
            .is_err()
    );
    store.record(&[observation("charge", None, 2000, 3)])?;
    let current = store.snapshot(&scope())?;
    assert_eq!(current.find(&key("charge")).ok_or("charge")?.value, None);
    assert_eq!(first.find(&key("charge")).ok_or("charge")?.value, Some(100));
    Ok(())
}
#[test]
fn concurrent_publishers_retain_complete_frozen_scope_snapshots()
-> Result<(), Box<dyn std::error::Error>> {
    let store = std::sync::Arc::new(Store::new(Limits {
        scopes: 2,
        observations_per_scope: 4,
    })?);
    let mut workers = Vec::new();
    for operation in ["charge", "balance"] {
        let store = store.clone();
        workers.push(std::thread::spawn(move || {
            // Busy is explicit: administrative callers may retry outside the guard.
            loop {
                match store.record(&[observation(operation, Some(100), 0, 1)]) {
                    Ok(()) => break,
                    Err(agw_invariants::cross_endpoint::Error::Busy) => std::thread::yield_now(),
                    Err(e) => panic!("{e}"),
                }
            }
        }));
    }
    for worker in workers {
        assert!(worker.join().is_ok());
    }
    assert_eq!(store.snapshot(&scope())?.observations().len(), 2);
    Ok(())
}

#[test]
fn every_scope_dimension_and_quantity_version_isolates_evidence()
-> Result<(), Box<dyn std::error::Error>> {
    let a = observation("charge", Some(100), 0, 1);
    for field in 0..10 {
        let mut b = observation("balance", Some(102), 0, 2);
        match field {
            0 => b.scope.tenant = "other".into(),
            1 => b.scope.connection = "other".into(),
            2 => b.scope.vendor = "other".into(),
            3 => b.scope.workflow = "other".into(),
            4 => b.scope.entity = "other".into(),
            5 => b.scope.revision = "other".into(),
            6 => b.key.concept = "other".into(),
            7 => b.key.unit = "other".into(),
            8 => b.key.contract_version = 2,
            _ => b.key.endpoint.side = Side::Request,
        }
        assert_eq!(
            compare(&rule(), Some(&a), Some(&b), 0)?,
            Reason::ScopeMismatch
        );
    }
    Ok(())
}
#[test]
fn failed_batch_rolls_back_prior_updates_and_preserves_capacity()
-> Result<(), Box<dyn std::error::Error>> {
    let store = Store::new(Limits {
        scopes: 1,
        observations_per_scope: 2,
    })?;
    store.record(&[observation("charge", Some(100), 0, 1)])?;
    let mut other = observation("balance", Some(100), 0, 1);
    other.scope.entity = "other".into();
    assert!(
        store
            .record(&[observation("charge", None, 1, 2), other])
            .is_err()
    );
    assert_eq!(
        store
            .snapshot(&scope())?
            .find(&key("charge"))
            .ok_or("charge")?
            .value,
        Some(100)
    );
    store.record(&[observation("balance", Some(100), 0, 1)])?;
    assert!(
        store
            .record(&[
                observation("balance", Some(100), 1, 2),
                observation("balance", Some(100), 2, 3)
            ])
            .is_err()
    );
    assert_eq!(
        store
            .snapshot(&scope())?
            .find(&key("balance"))
            .ok_or("balance")?
            .sequence,
        1
    );
    store.remove(&scope())?;
    assert!(store.snapshot(&scope())?.observations().is_empty());
    Ok(())
}

#[test]
fn background_replays_are_idempotent_but_conflicting_or_late_values_remain_stale()
-> Result<(), Box<dyn std::error::Error>> {
    let store = Store::new(Limits::default())?;
    let first = observation("balance", Some(i64::MAX), 100, 2);
    store.record_replayed(std::slice::from_ref(&first))?;
    store.record_replayed(std::slice::from_ref(&first))?;
    assert!(
        store
            .record_replayed(&[Observation {
                value: Some(0),
                ..first.clone()
            }])
            .is_err()
    );
    let newer = observation("balance", None, 101, 3);
    store.record_replayed(std::slice::from_ref(&newer))?;
    assert!(store.record_replayed(std::slice::from_ref(&first)).is_err());
    assert_eq!(
        store.snapshot(&scope())?.find(&key("balance")),
        Some(&newer)
    );
    Ok(())
}
#[test]
fn recipe_invalidation_removes_only_its_exact_current_value_and_keeps_a_sequence_fence()
-> Result<(), Box<dyn std::error::Error>> {
    let store = Store::new(Limits::default())?;
    let prior = observation("balance", Some(100), 100, 2);
    store.record(std::slice::from_ref(&prior))?;
    store.invalidate_if_current(&prior)?;
    let invalid = Observation {
        value: None,
        ..prior.clone()
    };
    assert_eq!(
        store.snapshot(&scope())?.find(&key("balance")),
        Some(&invalid)
    );
    assert!(store.record_replayed(std::slice::from_ref(&prior)).is_err());
    let newer = observation("balance", Some(200), 101, 3);
    store.record(std::slice::from_ref(&newer))?;
    store.invalidate_if_current(&prior)?;
    assert_eq!(
        store.snapshot(&scope())?.find(&key("balance")),
        Some(&newer)
    );
    Ok(())
}
