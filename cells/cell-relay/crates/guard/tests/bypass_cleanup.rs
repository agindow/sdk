//! TI-43: skipped inspection cleanup must not own the caller's outcome.
#[path = "support/cleanup.rs"]
mod cleanup;

use agw_guard::availability::{EngineFault, RegistrySnapshot, Status};
use agw_guard::config::{Config, RuntimeConfig};
use cleanup::{Cleanup, skipping_gate};
use std::panic::{AssertUnwindSafe, catch_unwind, panic_any};
use std::sync::atomic::{AtomicUsize, Ordering};

type Result = std::result::Result<(), Box<dyn std::error::Error>>;

#[test]
fn skipped_cleanup_panics_preserve_original_outcomes_in_every_mode() -> Result {
    for mode in ["observe", "repair", "strict"] {
        for open in [false, true] {
            for vendor_error in [false, true] {
                let call =
                    RuntimeConfig::from_sources(Config::default(), None)?.begin_call(Some(mode))?;
                let gate = skipping_gate(&call, open);
                let drops = AtomicUsize::new(0);
                let inspections = AtomicUsize::new(0);
                let calls = AtomicUsize::new(0);
                let original = vec![0, 255, 42];
                let resource = Cleanup {
                    drops: &drops,
                    panics: true,
                    payload_panics: false,
                };
                let evaluated = &inspections;
                let guarded = call.guard_with(
                    &gate,
                    &(),
                    RegistrySnapshot::Ready(&()),
                    0,
                    move |_, _, _| {
                        evaluated.fetch_add(1, Ordering::Relaxed);
                        drop(resource);
                        Ok(())
                    },
                    || {
                        calls.fetch_add(1, Ordering::Relaxed);
                        if vendor_error {
                            Err(&original)
                        } else {
                            Ok(&original)
                        }
                    },
                );
                assert_eq!(guarded.status, Status::Bypassed(EngineFault::Panic));
                let (Ok(value) | Err(value)) = guarded.outcome;
                assert!(std::ptr::eq(value, &original));
                assert_eq!(guarded.outcome.is_err(), vendor_error);
                assert_eq!(calls.load(Ordering::Relaxed), 1);
                assert_eq!(drops.load(Ordering::Relaxed), 1);
                assert_eq!(inspections.load(Ordering::Relaxed), 0);
                // Cleanup faults do not admit inspection or alter admission state.
                assert_eq!(
                    call.guard_with(
                        &gate,
                        &(),
                        RegistrySnapshot::Ready(&()),
                        0,
                        |_, _, _| panic!("cleanup altered admission"),
                        || (),
                    )
                    .status,
                    Status::Bypassed(if open {
                        EngineFault::CircuitOpen
                    } else {
                        EngineFault::NotReady
                    })
                );
            }
        }
    }
    Ok(())
}

#[test]
fn skipped_cleanup_finishes_before_original_operation_and_keeps_admission_reason() -> Result {
    let call = RuntimeConfig::from_sources(Config::default(), None)?.begin_call(None)?;
    for open in [false, true] {
        let drops = AtomicUsize::new(0);
        let resource = Cleanup {
            drops: &drops,
            panics: false,
            payload_panics: false,
        };
        let guarded = call.guard_with(
            &skipping_gate(&call, open),
            &(),
            RegistrySnapshot::Ready(&()),
            0,
            move |_, _, _| {
                drop(resource);
                panic!("inspection was not skipped");
            },
            || {
                assert_eq!(drops.load(Ordering::Relaxed), 1);
                17
            },
        );
        assert_eq!(guarded.outcome, 17);
        assert_eq!(
            guarded.status,
            Status::Bypassed(if open {
                EngineFault::CircuitOpen
            } else {
                EngineFault::NotReady
            })
        );
        assert_eq!(drops.load(Ordering::Relaxed), 1);
    }
    Ok(())
}

#[test]
fn skipped_cleanup_contains_panicking_payload_destructors() -> Result {
    let call = RuntimeConfig::from_sources(Config::default(), None)?.begin_call(None)?;
    for open in [false, true] {
        let drops = AtomicUsize::new(0);
        let resource = Cleanup {
            drops: &drops,
            panics: true,
            payload_panics: true,
        };
        let guarded = call.guard_with(
            &skipping_gate(&call, open),
            &(),
            RegistrySnapshot::Ready(&()),
            0,
            move |_, _, _| {
                drop(resource);
                panic!("inspection was not skipped");
            },
            || 23,
        );
        assert_eq!(guarded.status, Status::Bypassed(EngineFault::Panic));
        assert_eq!(guarded.outcome, 23);
        assert_eq!(drops.load(Ordering::Relaxed), 1);
    }
    Ok(())
}

#[test]
fn skipped_cleanup_precedes_and_preserves_original_panics() -> Result {
    let call = RuntimeConfig::from_sources(Config::default(), None)?.begin_call(None)?;
    for panics in [false, true] {
        for open in [false, true] {
            let gate = skipping_gate(&call, open);
            let drops = AtomicUsize::new(0);
            let calls = AtomicUsize::new(0);
            let resource = Cleanup {
                drops: &drops,
                panics,
                payload_panics: false,
            };
            let outcome = catch_unwind(AssertUnwindSafe(|| {
                call.guard_with(
                    &gate,
                    &(),
                    RegistrySnapshot::Ready(&()),
                    0,
                    move |_, _, _| {
                        drop(resource);
                        panic!("inspection was not skipped");
                    },
                    || {
                        calls.fetch_add(1, Ordering::Relaxed);
                        assert_eq!(drops.load(Ordering::Relaxed), 1);
                        panic_any(73_u32);
                    },
                );
            }));
            match outcome {
                Err(payload) => assert_eq!(payload.downcast_ref::<u32>(), Some(&73)),
                Ok(_) => panic!("swallowed caller panic"),
            }
            assert_eq!(calls.load(Ordering::Relaxed), 1);
            assert_eq!(drops.load(Ordering::Relaxed), 1);
        }
    }
    Ok(())
}
