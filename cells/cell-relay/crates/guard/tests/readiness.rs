//! TI-42: benchmark only real guard branches, with deterministic clock control.
use agw_guard::availability::{EngineFault, RegistrySnapshot, Status};
use agw_guard::config::{Config, RuntimeConfig};
use agw_guard::readiness::{AvailabilityGate, Clock};
use std::sync::{
    Arc,
    atomic::{AtomicU64, AtomicUsize, Ordering},
};
use std::time::Duration;

#[derive(Clone, Default)]
struct ManualClock(Arc<AtomicU64>);
impl Clock for ManualClock {
    fn now(&self) -> Duration {
        Duration::from_millis(self.0.load(Ordering::Relaxed))
    }
}
impl ManualClock {
    fn set(&self, millis: u64) {
        self.0.store(millis, Ordering::Relaxed);
    }
}
type Result = std::result::Result<(), Box<dyn std::error::Error>>;

#[test]
fn not_ready_skips_inspection_and_preserves_identity_in_every_mode() -> Result {
    for mode in ["observe", "repair", "strict"] {
        let call = RuntimeConfig::from_sources(Config::default(), None)?.begin_call(Some(mode))?;
        let gate = AvailabilityGate::new(false);
        let original = vec![0, 255, 42];
        let result = call.guard_with(
            &gate,
            &(),
            RegistrySnapshot::Ready(&()),
            0,
            |_, _, _| panic!("not-ready engine evaluated"),
            || &original,
        );
        assert_eq!(result.status, Status::Bypassed(EngineFault::NotReady));
        assert!(std::ptr::eq(result.outcome, &original));
        gate.set_ready(true);
        assert_eq!(
            call.guard_with(
                &gate,
                &(),
                RegistrySnapshot::Ready(&()),
                0,
                |_, _, _| Ok(()),
                || ()
            )
            .status,
            Status::Inspected
        );
    }
    Ok(())
}

#[test]
fn failures_open_shared_cooldown_failed_probe_renews_and_success_recovers() -> Result {
    let call = RuntimeConfig::from_sources(Config::default(), None)?.begin_call(None)?;
    let clock = ManualClock::default();
    let gate = AvailabilityGate::with_clock(true, clock.clone());
    let clone = gate.clone();
    let evaluations = AtomicUsize::new(0);
    let fail = |_: &(), _: &(), _: &mut agw_guard::availability::Budget| {
        evaluations.fetch_add(1, Ordering::Relaxed);
        Err(EngineFault::Internal)
    };
    for _ in 0..3 {
        assert_eq!(
            call.guard_with(&gate, &(), RegistrySnapshot::Ready(&()), 0, fail, || 7)
                .outcome,
            7
        );
    }
    clock.set(999);
    let original_error = String::from("vendor error");
    let skipped = call.guard_with(&clone, &(), RegistrySnapshot::Ready(&()), 0, fail, || {
        Err::<(), _>(&original_error)
    });
    assert_eq!(skipped.status, Status::Bypassed(EngineFault::CircuitOpen));
    assert!(matches!(skipped.outcome, Err(error) if std::ptr::eq(error, &original_error)));
    assert_eq!(evaluations.load(Ordering::Relaxed), 3);
    clock.set(1000);
    assert_eq!(
        call.guard_with(&gate, &(), RegistrySnapshot::Ready(&()), 0, fail, || ())
            .status,
        Status::Bypassed(EngineFault::Internal)
    );
    clock.set(1999);
    assert_eq!(
        call.guard_with(&gate, &(), RegistrySnapshot::Ready(&()), 0, fail, || ())
            .status,
        Status::Bypassed(EngineFault::CircuitOpen)
    );
    assert_eq!(evaluations.load(Ordering::Relaxed), 4);
    clock.set(2000);
    assert_eq!(
        call.guard_with(
            &gate,
            &(),
            RegistrySnapshot::Ready(&()),
            0,
            |_, _, _| Ok(()),
            || ()
        )
        .status,
        Status::Inspected
    );
    assert_eq!(
        call.guard_with(&gate, &(), RegistrySnapshot::Ready(&()), 0, fail, || ())
            .status,
        Status::Bypassed(EngineFault::Internal)
    );
    assert_eq!(evaluations.load(Ordering::Relaxed), 5);
    Ok(())
}

#[test]
fn success_resets_failures_and_engine_instances_are_independent() -> Result {
    let call = RuntimeConfig::from_sources(Config::default(), None)?.begin_call(None)?;
    let gate = AvailabilityGate::new(true);
    for success in [false, false, true, false, false, true] {
        let result = call.guard_with(
            &gate,
            &(),
            RegistrySnapshot::Ready(&()),
            0,
            |_, _, _| {
                if success {
                    Ok(())
                } else {
                    Err(EngineFault::Internal)
                }
            },
            || (),
        );
        assert_ne!(result.status, Status::Bypassed(EngineFault::CircuitOpen));
    }
    for _ in 0..3 {
        call.guard_with(
            &gate,
            &(),
            RegistrySnapshot::Ready(&()),
            0,
            |_, _, _| Err(EngineFault::Internal),
            || (),
        );
    }
    let other = AvailabilityGate::new(true);
    assert_eq!(
        call.guard_with(
            &other,
            &(),
            RegistrySnapshot::Ready(&()),
            0,
            |_, _, _| Ok(()),
            || ()
        )
        .status,
        Status::Inspected
    );
    Ok(())
}

#[test]
fn expired_circuit_admits_only_one_probe_and_old_success_cannot_close_it() -> Result {
    let call = RuntimeConfig::from_sources(Config::default(), None)?.begin_call(None)?;
    let clock = ManualClock::default();
    let gate = AvailabilityGate::with_clock(true, clock.clone());
    // Start a healthy evaluation, then open the circuit before it completes.
    call.guard_with(
        &gate,
        &(),
        RegistrySnapshot::Ready(&()),
        0,
        |_, _, _| {
            for _ in 0..3 {
                call.guard_with(
                    &gate,
                    &(),
                    RegistrySnapshot::Ready(&()),
                    0,
                    |_, _, _| Err(EngineFault::Internal),
                    || (),
                );
            }
            Ok(())
        },
        || (),
    );
    assert_eq!(
        call.guard_with(
            &gate,
            &(),
            RegistrySnapshot::Ready(&()),
            0,
            |_, _, _| panic!("old success closed circuit"),
            || ()
        )
        .status,
        Status::Bypassed(EngineFault::CircuitOpen)
    );
    clock.set(1000);
    let probed = call.guard_with(
        &gate,
        &(),
        RegistrySnapshot::Ready(&()),
        0,
        |_, _, _| {
            let competing = call.guard_with(
                &gate,
                &(),
                RegistrySnapshot::Ready(&()),
                0,
                |_, _, _| panic!("second probe admitted"),
                || 11,
            );
            assert_eq!(competing.status, Status::Bypassed(EngineFault::CircuitOpen));
            assert_eq!(competing.outcome, 11);
            Ok(())
        },
        || 13,
    );
    assert_eq!(probed.status, Status::Inspected);
    assert_eq!(probed.outcome, 13);
    Ok(())
}

#[test]
fn vendor_errors_never_count_as_engine_failures() -> Result {
    let call = RuntimeConfig::from_sources(Config::default(), None)?.begin_call(None)?;
    let gate = AvailabilityGate::new(true);
    let vendor_error = String::from("vendor failure");
    let inspections = AtomicUsize::new(0);
    for _ in 0..4 {
        let result = call.guard_with(
            &gate,
            &(),
            RegistrySnapshot::Ready(&()),
            0,
            |_, _, _| {
                inspections.fetch_add(1, Ordering::Relaxed);
                Ok(())
            },
            || Err::<(), _>(&vendor_error),
        );
        assert_eq!(result.status, Status::Inspected);
        assert!(matches!(result.outcome, Err(error) if std::ptr::eq(error, &vendor_error)));
    }
    assert_eq!(inspections.load(Ordering::Relaxed), 4);
    Ok(())
}
