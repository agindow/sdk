use agw_guard::credential::{Cause, Evidence, cutover::*};
use std::sync::Arc;
#[path = "support/credential_cutover.rs"]
mod fixture;
use fixture::{machine, probe};
#[test]
fn probes_fence_and_overlap_retire_exactly_once() -> Result<(), Box<dyn std::error::Error>> {
    let mut c = machine()?;
    c.start(0);
    assert_eq!(c.phase(), Phase::Canary);
    assert_eq!(*c.dispatch(), "active");
    let in_flight = c.dispatch();
    assert!(c.record_probe(probe(None), 1));
    assert!(c.promote(2));
    assert_eq!(*c.dispatch(), "candidate");
    assert_eq!(*in_flight, "active");
    assert!(!c.tick(51));
    assert!(c.tick(52));
    assert!(!c.tick(53));
    assert_eq!(c.phase(), Phase::Retired);
    assert_eq!(c.receipt().window_end, Some(52));
    Ok(())
}
#[test]
fn write_gate_quota_and_failures_never_promote() -> Result<(), Box<dyn std::error::Error>> {
    let mut c = machine()?;
    c.start(0);
    let mut write = probe(None);
    write.kind = Kind::Write;
    assert!(!c.admit(&write));
    assert!(!c.record_probe(write, 1));
    assert_eq!(c.phase(), Phase::Aborted);
    assert!(!c.promote(2));
    c.start(3);
    assert_eq!(c.phase(), Phase::Aborted);
    let mut c = machine()?;
    c.start(0);
    assert!(c.record_probe(probe(Some(Cause::Quota)), 1));
    assert!(!c.promote(2));
    assert!(c.record_probe(probe(None), 3));
    assert!(c.promote(4));
    for n in 1..=3 {
        c.failure(Cause::Rejected, Evidence::Unknown, "write", "audit-2", n);
        assert_eq!(
            c.phase(),
            if n < 3 {
                Phase::Overlap
            } else {
                Phase::Reverted
            }
        );
        assert_eq!(*c.dispatch(), if n < 3 { "candidate" } else { "active" });
    }
    assert_eq!(c.phase(), Phase::Reverted);
    assert_eq!(*c.dispatch(), "active");
    assert!(!c.promote(5));
    c.revert();
    assert_eq!(c.phase(), Phase::Reverted);
    Ok(())
}
#[test]
fn missing_or_corrupt_restart_freezes_active() -> Result<(), Box<dyn std::error::Error>> {
    let mut c = machine()?;
    c.start(0);
    c.record_probe(probe(None), 1);
    c.promote(2);
    let snapshot = c.snapshot()?;
    let restored = Cutover::restore(machine()?, Some(&snapshot));
    assert_eq!(restored.phase(), Phase::Overlap);
    for source in [None, Some("invalid"), Some("{}")] {
        let restored = Cutover::restore(machine()?, source);
        assert_eq!(restored.phase(), Phase::Unknown);
        assert_eq!(*restored.dispatch(), "active");
    }
    Ok(())
}
#[test]
fn partial_plan_and_atomic_providers_never_switch() -> Result<(), Box<dyn std::error::Error>> {
    let mut c = machine()?;
    assert!(c.set_plan(vec![
        Definition {
            operation: "read".into(),
            kind: Kind::Read,
            write_opt_in: false,
            idempotency_key_present: false
        },
        Definition {
            operation: "write".into(),
            kind: Kind::Write,
            write_opt_in: true,
            idempotency_key_present: true
        },
    ]));
    c.start(0);
    c.record_probe(probe(None), 1);
    assert!(!c.promote(2));
    assert_eq!(*c.dispatch(), "active");
    let mut c = Cutover::new(
        "atomic",
        Arc::new("active"),
        Arc::new("candidate"),
        Config {
            dual_key: Some(false),
            max_probes: 1,
            deadline: 10,
            overlap_ms: 10,
            threshold: 3,
        },
    )?;
    c.set_plan(vec![Definition {
        operation: "read".into(),
        kind: Kind::Read,
        write_opt_in: false,
        idempotency_key_present: false,
    }]);
    c.start(0);
    assert_eq!(c.phase(), Phase::Unsupported);
    assert!(c.candidate(&probe(None)).is_none());
    Ok(())
}
#[test]
fn ten_thousand_dispatches_capture_one_binding() -> Result<(), Box<dyn std::error::Error>> {
    let mut c = machine()?;
    c.start(0);
    c.record_probe(probe(None), 1);
    let mut handles = Vec::new();
    for n in 0..10000 {
        if n == 5000 {
            c.promote(2);
        }
        handles.push(c.dispatch());
    }
    assert!(handles[..5000].iter().all(|b| **b == "active"));
    assert!(handles[5000..].iter().all(|b| **b == "candidate"));
    c.revert();
    assert_eq!(*c.dispatch(), "active");
    Ok(())
}
#[test]
fn detached_dispatch_races_operator_without_locking() -> Result<(), Box<dyn std::error::Error>> {
    let mut c = machine()?;
    c.start(0);
    c.record_probe(probe(None), 1);
    let dispatch = Arc::new(c.dispatcher());
    let mut workers = Vec::new();
    for _ in 0..4 {
        let dispatch = Arc::clone(&dispatch);
        workers.push(std::thread::spawn(move || {
            for _ in 0..2500 {
                let binding = dispatch.dispatch();
                assert!(matches!(*binding, "active" | "candidate"));
            }
        }));
    }
    c.promote(2);
    c.revert();
    for worker in workers {
        assert!(worker.join().is_ok());
    }
    assert_eq!(*dispatch.dispatch(), "active");
    Ok(())
}
#[test]
fn restart_preserves_pre_promotion_phases() -> Result<(), Box<dyn std::error::Error>> {
    let mut c = machine()?;
    assert_eq!(
        Cutover::restore(machine()?, Some(&c.snapshot()?)).phase(),
        Phase::Idle
    );
    c.start(0);
    assert_eq!(
        Cutover::restore(machine()?, Some(&c.snapshot()?)).phase(),
        Phase::Canary
    );
    c.abort();
    assert_eq!(
        Cutover::restore(machine()?, Some(&c.snapshot()?)).phase(),
        Phase::Aborted
    );
    Ok(())
}
#[test]
fn missing_restart_state_resets_an_existing_candidate_selection()
-> Result<(), Box<dyn std::error::Error>> {
    let mut c = machine()?;
    c.start(0);
    c.record_probe(probe(None), 1);
    c.promote(2);
    let dispatcher = c.dispatcher();
    assert_eq!(*dispatcher.dispatch(), "candidate");
    let restored = Cutover::restore(c, None);
    assert_eq!(restored.phase(), Phase::Unknown);
    assert_eq!(*dispatcher.dispatch(), "active");
    Ok(())
}
#[test]
fn canary_binding_fault_aborts_and_release_callback_is_once()
-> Result<(), Box<dyn std::error::Error>> {
    let mut c = machine()?;
    c.start(0);
    assert!(!c.run_probe(&probe(None), 1, |_| Err(())));
    assert_eq!(c.phase(), Phase::Aborted);
    assert_eq!(*c.dispatch(), "active");
    let mut c = machine()?;
    c.start(0);
    c.record_probe(probe(None), 1);
    c.promote(2);
    let mut releases = 0;
    assert!(c.tick_with_release(52, || releases += 1));
    assert!(!c.tick_with_release(53, || releases += 1));
    assert_eq!(releases, 1);
    Ok(())
}

#[test]
fn non_read_plans_require_opt_in_and_idempotency_before_any_probe()
-> Result<(), Box<dyn std::error::Error>> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    for kind in [Kind::Write, Kind::Unknown] {
        for (opt_in, key) in [(false, false), (false, true), (true, false)] {
            let mut c = machine()?;
            assert!(!c.set_plan(vec![Definition {
                operation: "read".into(),
                kind,
                write_opt_in: opt_in,
                idempotency_key_present: key,
            }]));
            c.start(0);
            let mut definition = probe(None);
            definition.kind = kind;
            definition.write_opt_in = opt_in;
            definition.idempotency_key_present = key;
            let calls = AtomicUsize::new(0);
            assert!(!c.run_probe(&definition, 1, |_| {
                calls.fetch_add(1, Ordering::Relaxed);
                Ok(probe(None))
            }));
            assert_eq!(calls.load(Ordering::Relaxed), 0);
            assert_eq!(*c.dispatch(), "active");
        }
    }
    Ok(())
}

#[test]
fn authoritative_terminal_failures_revert_without_waiting_for_the_threshold()
-> Result<(), Box<dyn std::error::Error>> {
    for (cause, evidence) in [
        (Cause::Expired, Evidence::ProviderDeclared),
        (Cause::Revoked, Evidence::HostDeclared),
    ] {
        let mut c = machine()?;
        c.start(0);
        c.record_probe(probe(None), 1);
        c.promote(2);
        c.failure(Cause::Quota, Evidence::ProviderDeclared, "read", "quota", 3);
        assert_eq!(c.receipt().failure_count, 0);
        c.failure(cause, evidence, "read", "terminal", 4);
        assert_eq!(c.phase(), Phase::Reverted);
        assert_eq!(c.receipt().failure_count, 1);
        assert_eq!(*c.dispatch(), "active");
    }
    Ok(())
}
#[test]
fn receipts_reject_secret_labels_and_never_serialize_opaque_binding_values()
-> Result<(), Box<dyn std::error::Error>> {
    let secret = "sk_live_AGINDOWcanary123";
    assert!(
        Cutover::new(
            secret,
            Arc::new("active"),
            Arc::new("candidate"),
            Config::default()
        )
        .is_err()
    );
    let mut c = Cutover::new(
        "redaction",
        Arc::new(secret),
        Arc::new(secret),
        Config {
            dual_key: Some(true),
            ..Config::default()
        },
    )?;
    assert!(!c.set_plan(vec![Definition {
        operation: secret.into(),
        kind: Kind::Read,
        write_opt_in: false,
        idempotency_key_present: false
    }]));
    assert!(c.set_plan(vec![Definition {
        operation: "read".into(),
        kind: Kind::Read,
        write_opt_in: false,
        idempotency_key_present: false
    }]));
    c.start(0);
    let mut unsafe_probe = probe(None);
    unsafe_probe.audit_ref = secret.into();
    assert!(!c.admit(&unsafe_probe));
    c.record_probe(probe(None), 1);
    assert!(c.promote(2));
    c.failure(
        Cause::Expired,
        Evidence::ProviderDeclared,
        secret,
        secret,
        3,
    );
    assert_eq!(c.phase(), Phase::Reverted);
    assert!(c.receipt().canary);
    assert!(c.receipt().failing_operation.is_none());
    assert!(c.receipt().audit_ref.is_none());
    assert!(!serde_json::to_string(c.receipt())?.contains(secret));
    Ok(())
}

#[test]
fn inconclusive_http_probes_consume_a_bounded_budget_without_claiming_credential_failure()
-> Result<(), Box<dyn std::error::Error>> {
    let mut c = machine()?;
    c.start(0);
    for (now, status) in [(1, 500), (2, 403)] {
        let mut transient = probe(None);
        transient.status = status;
        assert!(c.record_probe(transient, now));
        assert_eq!(c.phase(), Phase::Canary);
        assert!(!c.promote(now));
    }
    assert!(c.record_probe(probe(None), 3));
    assert!(c.promote(4));
    let mut c = machine()?;
    c.start(0);
    for now in 1..=4 {
        let mut transient = probe(None);
        transient.status = 503;
        assert_eq!(c.record_probe(transient, now), now < 4);
    }
    assert_eq!(c.phase(), Phase::Aborted);
    assert_eq!(c.receipt().probes.len(), 4);
    assert_eq!(*c.dispatch(), "active");
    Ok(())
}

#[test]
fn unsupported_cutovers_record_whether_capability_is_unknown_or_atomically_invalidating()
-> Result<(), Box<dyn std::error::Error>> {
    for (dual_key, reason) in [
        (Some(false), "atomic_invalidation"),
        (None, "unknown_capability"),
    ] {
        let mut c = Cutover::new(
            "unsupported",
            Arc::new("active"),
            Arc::new("candidate"),
            Config {
                dual_key,
                ..Config::default()
            },
        )?;
        c.set_plan(vec![Definition {
            operation: "read".into(),
            kind: Kind::Read,
            write_opt_in: false,
            idempotency_key_present: false,
        }]);
        c.start(0);
        assert_eq!(c.phase(), Phase::Unsupported);
        assert_eq!(
            serde_json::to_value(c.receipt())?["unsupported_reason"],
            reason
        );
        assert_eq!(*c.dispatch(), "active");
        assert!(!c.promote(1));
    }
    Ok(())
}
