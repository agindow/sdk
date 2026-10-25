//! TI-40: inspection faults must never own the caller's outcome.
use std::cell::Cell;
use std::panic::{AssertUnwindSafe, catch_unwind, panic_any};
use std::sync::atomic::{AtomicUsize, Ordering};

use agw_guard::availability::{Budget, EngineFault, RegistrySnapshot, Status};
use agw_guard::config::{Config, RuntimeConfig};

type Result = std::result::Result<(), Box<dyn std::error::Error>>;

#[test]
fn internal_faults_preserve_identity_and_run_operation_once_in_every_mode() -> Result {
    for mode in ["observe", "repair", "strict"] {
        for fault in [EngineFault::Internal, EngineFault::ResourceExhausted] {
            let call =
                RuntimeConfig::from_sources(Config::default(), None)?.begin_call(Some(mode))?;
            let input = vec![0, 255, 42];
            let output = vec![42, 255, 0];
            let calls = Cell::new(0);
            let guarded = call.guard(
                &input,
                RegistrySnapshot::Ready(&()),
                10,
                |seen, _, _| {
                    assert!(std::ptr::eq(seen, &input));
                    Err(fault)
                },
                || {
                    calls.set(calls.get() + 1);
                    &output
                },
            );
            assert_eq!(guarded.status, Status::Bypassed(fault));
            assert!(std::ptr::eq(guarded.outcome, &output));
            assert_eq!(calls.get(), 1);
        }
    }
    Ok(())
}

#[test]
fn corrupt_or_missing_local_registry_skips_engine_and_preserves_upstream_error() -> Result {
    let call = RuntimeConfig::from_sources(Config::default(), None)?.begin_call(None)?;
    let original_error = String::from("vendor rejected request");
    for (registry, reason) in [
        (
            RegistrySnapshot::<()>::Corrupt,
            EngineFault::RegistryCorrupt,
        ),
        (
            RegistrySnapshot::Unavailable,
            EngineFault::RegistryUnavailable,
        ),
    ] {
        let guarded = call.guard(
            &(),
            registry,
            10,
            |_, _, _| panic!("must skip engine"),
            || Err::<(), _>(&original_error),
        );
        assert_eq!(guarded.status, Status::Bypassed(reason));
        match guarded.outcome {
            Err(error) => assert!(std::ptr::eq(error, &original_error)),
            Ok(()) => panic!("lost vendor error"),
        }
    }
    Ok(())
}

#[test]
fn exhaustion_stops_work_before_allocation_and_is_sticky() -> Result {
    let call = RuntimeConfig::from_sources(Config::default(), None)?.begin_call(None)?;
    let work = AtomicUsize::new(0);
    let guarded = call.guard(
        &(),
        RegistrySnapshot::Ready(&()),
        4,
        |_, _, budget| {
            for _ in 0..10 {
                budget.charge(2)?;
                work.fetch_add(1, Ordering::Relaxed);
            }
            Ok(())
        },
        || 123,
    );
    assert_eq!(work.load(Ordering::Relaxed), 2);
    assert_eq!(
        guarded.status,
        Status::Bypassed(EngineFault::ResourceExhausted)
    );
    assert_eq!(guarded.outcome, 123);
    let ignored = call.guard(
        &(),
        RegistrySnapshot::Ready(&()),
        0,
        |_, _, budget| {
            let _ = budget.charge(1);
            Ok(())
        },
        || 456,
    );
    assert_eq!(
        ignored.status,
        Status::Bypassed(EngineFault::ResourceExhausted)
    );
    assert_eq!(ignored.outcome, 456);
    let mut budget = Budget::new(usize::MAX);
    assert_eq!(budget.charge(usize::MAX), Ok(()));
    assert_eq!(budget.remaining(), 0);
    assert_eq!(budget.charge(1), Err(EngineFault::ResourceExhausted));
    assert_eq!(budget.charge(0), Err(EngineFault::ResourceExhausted));
    Ok(())
}

#[test]
fn successful_inspection_and_exact_budget_preserve_original_outcome() -> Result {
    let call = RuntimeConfig::from_sources(Config::default(), None)?.begin_call(None)?;
    let registry = vec![1, 2, 3];
    let guarded = call.guard(
        &(),
        RegistrySnapshot::Ready(&registry),
        3,
        |_, seen, budget| {
            assert!(std::ptr::eq(seen, &registry));
            budget.charge(3)
        },
        || 17,
    );
    assert_eq!(guarded.status, Status::Inspected);
    assert_eq!(guarded.outcome, 17);
    Ok(())
}

#[test]
fn engine_panic_is_contained_and_next_call_remains_usable() -> Result {
    let call = RuntimeConfig::from_sources(Config::default(), None)?.begin_call(None)?;
    let guarded = call.guard(
        &(),
        RegistrySnapshot::Ready(&()),
        1,
        |_, _, _| panic!("injected engine panic"),
        || 29,
    );
    assert_eq!(guarded.status, Status::Bypassed(EngineFault::Panic));
    assert_eq!(guarded.outcome, 29);
    let next = call.guard(
        &(),
        RegistrySnapshot::Ready(&()),
        1,
        |_, _, _| Ok(()),
        || 31,
    );
    assert_eq!(next.status, Status::Inspected);
    assert_eq!(next.outcome, 31);
    Ok(())
}

#[test]
fn panicking_payload_destructor_cannot_escape_recovery() -> Result {
    struct BadPayload;
    impl Drop for BadPayload {
        fn drop(&mut self) {
            panic!("injected destructor panic");
        }
    }
    let call = RuntimeConfig::from_sources(Config::default(), None)?.begin_call(None)?;
    let guarded = call.guard(
        &(),
        RegistrySnapshot::Ready(&()),
        0,
        |_, _, _| panic_any(BadPayload),
        || 37,
    );
    assert_eq!(guarded.status, Status::Bypassed(EngineFault::Panic));
    assert_eq!(guarded.outcome, 37);
    Ok(())
}

#[test]
fn callers_panic_is_never_swallowed_or_retried() -> Result {
    let call = RuntimeConfig::from_sources(Config::default(), None)?.begin_call(None)?;
    let calls = Cell::new(0);
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        call.guard(
            &(),
            RegistrySnapshot::Ready(&()),
            0,
            |_, _, _| Err(EngineFault::Internal),
            || {
                calls.set(calls.get() + 1);
                panic_any(41_u32);
            },
        );
    }));
    match outcome {
        Err(payload) => assert_eq!(payload.downcast_ref::<u32>(), Some(&41)),
        Ok(()) => panic!("swallowed caller panic"),
    }
    assert_eq!(calls.get(), 1);
    Ok(())
}
