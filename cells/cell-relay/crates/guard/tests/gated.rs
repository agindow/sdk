#[path = "support/gated.rs"]
mod support;
use agw_diff::{DriftClass as Class, Side};
use agw_guard::{
    config::{Config, RuntimeConfig},
    gated::{Body, Call},
    readiness::AvailabilityGate,
};
use agw_policy::{Decision, Mode, Operation};
use std::sync::atomic::Ordering;
use support::{Engine, draft};
type TestResult = Result<(), Box<dyn std::error::Error>>;

static SOURCE: &[u8] = b"{ \"old\" : 1e0, \"untouched\": 1.00 }";
static PATCHED: &[u8] = b"{ \"new\" : 1e0, \"untouched\": 1.00 }";
fn config(mode: Mode) -> Result<RuntimeConfig, Box<dyn std::error::Error>> {
    RuntimeConfig::from_sources(
        Config {
            mode: Some(mode.to_string()),
            ..Config::default()
        },
        None,
    )
    .map_err(Into::into)
}

#[test]
fn full_matrix_controls_dispatch_and_effective_bytes() -> TestResult {
    for mode in [Mode::Observe, Mode::Repair, Mode::Strict] {
        for op in [Operation::Read, Operation::Write, Operation::Unknown] {
            for class in [
                None,
                Some(Class::Shape),
                Some(Class::Semantic),
                Some(Class::Unknown),
            ] {
                for repair in [0, 1, 2] {
                    let mut engine = Engine::new(class);
                    // Shape rename and same-type semantic scalar replacements are supported.
                    engine.propose = repair > 0 && class.is_some();
                    engine.verify = repair == 2;
                    let runtime = config(mode)?;
                    let mut audit = draft()?;
                    audit.operation = op;
                    let gate = AvailabilityGate::new(true);
                    let mut invoked = 0;
                    let result = Call::new(runtime.begin_call(None)?, &gate, 100_000).execute(
                        Body::complete(SOURCE),
                        &engine,
                        audit,
                        |sent| {
                            invoked += 1;
                            assert_eq!(
                                sent,
                                if engine.propose
                                    && engine.verify
                                    && class != Some(Class::Unknown)
                                    && mode != Mode::Observe
                                {
                                    if class == Some(Class::Shape) {
                                        PATCHED
                                    } else {
                                        b"{ \"old\" : 2, \"untouched\": 1.00 }"
                                    }
                                } else {
                                    SOURCE
                                }
                            );
                            (17, Body::complete(SOURCE))
                        },
                    );
                    let expected = if mode == Mode::Observe || class.is_none() {
                        Decision::Pass
                    } else if engine.propose && engine.verify && class != Some(Class::Unknown) {
                        Decision::Repaired
                    } else if mode == Mode::Strict
                        || (op != Operation::Read && class != Some(Class::Shape))
                    {
                        Decision::Halt
                    } else {
                        Decision::Degraded
                    };
                    assert_eq!(
                        result.request.decision, expected,
                        "{mode:?}/{op:?}/{class:?}/{repair}"
                    );
                    assert_eq!(invoked, usize::from(expected != Decision::Halt));
                    assert_eq!(result.bypasses, 0);
                    assert_eq!(result.audit.mode, mode);
                    assert!(result.audit.detail.is_some());
                    assert_eq!(
                        result.headers.get("X-Agindow-Status"),
                        Some(result.audit.decision.to_string().as_str())
                    );
                    if mode == Mode::Observe {
                        assert!(std::ptr::eq(result.request.body.as_ref(), SOURCE));
                        assert!(std::ptr::eq(
                            result
                                .response
                                .as_ref()
                                .ok_or("expected outcome")?
                                .body
                                .as_ref(),
                            SOURCE
                        ));
                        assert_eq!(engine.verifications.load(Ordering::SeqCst), 0);
                        assert!(result.audit.patch.is_none());
                    }
                }
            }
        }
    }
    Ok(())
}

#[test]
fn verified_selective_repairs_compose_both_sides_without_inventing_write_proof() -> TestResult {
    let runtime = config(Mode::Repair)?;
    let mut engine = Engine::new(Some(Class::Shape));
    engine.propose = true;
    let gate = AvailabilityGate::new(true);
    let result = Call::new(runtime.begin_call(None)?, &gate, 100_000).execute(
        Body::complete(SOURCE),
        &engine,
        draft()?,
        |sent| {
            assert_eq!(sent, PATCHED);
            (Box::new(7), Body::complete(SOURCE))
        },
    );
    assert_eq!(
        result.response.ok_or("expected outcome")?.body.as_ref(),
        PATCHED
    );
    assert_eq!(*result.outcome.ok_or("expected outcome")?, 7);
    let patch = result.audit.patch.ok_or("expected outcome")?;
    assert!(patch.applied && !patch.byte_identical);
    assert_eq!(
        patch
            .fields_changed
            .iter()
            .map(|field| field.as_str().to_owned())
            .collect::<Vec<_>>(),
        ["request:old", "response:old"]
    );
    assert_eq!(
        result
            .audit
            .drift
            .ok_or("expected outcome")?
            .side
            .to_string(),
        "both"
    );
    assert!(!result.audit.proof.write_integrity);
    assert_eq!(result.headers.get("X-Agindow-Repair"), Some("applied"));
    Ok(())
}

#[test]
fn availability_dominates_policy_and_preserves_completed_outcome() -> TestResult {
    for mode in [Mode::Observe, Mode::Repair, Mode::Strict] {
        for side in [Side::Request, Side::Response] {
            for panic in [false, true] {
                let runtime = config(mode)?;
                let mut engine = Engine::new(Some(Class::Shape));
                engine.propose = true;
                engine.fault = Some(side);
                engine.panic = panic;
                let gate = AvailabilityGate::new(true);
                let original = Box::new(31);
                let ptr = &*original as *const i32;
                let result = Call::new(runtime.begin_call(None)?, &gate, 100_000).execute(
                    Body::complete(SOURCE),
                    &engine,
                    draft()?,
                    |_| (original, Body::complete(SOURCE)),
                );
                assert_eq!(result.bypasses, 1);
                assert_eq!(result.audit.decision.to_string(), "bypassed");
                assert_eq!(
                    &*result.outcome.ok_or("expected outcome")? as *const i32,
                    ptr
                );
                if side == Side::Request {
                    assert_eq!(engine.evaluations.load(Ordering::SeqCst), 1);
                    assert!(result.response.is_none());
                    assert!(result.audit.patch.is_none());
                } else {
                    assert_eq!(
                        result.response.ok_or("expected outcome")?.body.as_ref(),
                        SOURCE
                    );
                    if mode == Mode::Observe {
                        assert!(result.audit.patch.is_none());
                    } else {
                        assert!(result.audit.patch.ok_or("expected outcome")?.applied);
                    }
                }
            }
        }
    }
    Ok(())
}

#[test]
fn reload_override_and_bypass_keep_one_mode_snapshot() -> TestResult {
    let runtime = config(Mode::Strict)?;
    let engine = Engine::new(Some(Class::Semantic));
    let gate = AvailabilityGate::new(true);
    let context = runtime.begin_call(Some("observe"))?;
    let result = Call::new(context, &gate, 100_000).execute(
        Body::complete(SOURCE),
        &engine,
        draft()?,
        |_| {
            assert!(runtime.reload_mode("repair").is_ok());
            (99, Body::complete(SOURCE))
        },
    );
    assert_eq!(result.request.decision, Decision::Pass);
    assert_eq!(
        result.response.ok_or("expected outcome")?.decision,
        Decision::Pass
    );
    assert_eq!(result.headers.get("X-Agindow-Mode"), Some("observe"));
    assert_eq!(result.audit.mode, Mode::Observe);
    assert_eq!(runtime.begin_call(None)?.mode(), Mode::Repair);
    gate.set_ready(false);
    let result = Call::new(runtime.begin_call(None)?, &gate, 100_000).execute(
        Body::complete(SOURCE),
        &engine,
        draft()?,
        |_| {
            assert!(runtime.reload_mode("strict").is_ok());
            (99, Body::complete(SOURCE))
        },
    );
    assert_eq!(result.audit.mode, Mode::Repair);
    assert_eq!(result.headers.get("X-Agindow-Mode"), Some("repair"));
    assert_eq!(result.bypasses, 1);
    Ok(())
}

#[test]
fn strict_uses_its_snapshot_for_the_higher_repair_threshold() -> TestResult {
    for mode in [Mode::Repair, Mode::Strict] {
        let runtime = config(mode)?;
        let gate = AvailabilityGate::new(true);
        let mut engine = Engine::new(Some(Class::Shape));
        engine.propose = true;
        engine.confidence = 0.98;
        let result = Call::new(runtime.begin_call(None)?, &gate, 100_000).execute(
            Body::complete(SOURCE),
            &engine,
            draft()?,
            |sent| {
                assert_eq!(sent, PATCHED);
                (5, Body::complete(SOURCE))
            },
        );
        assert_eq!(result.outcome.is_some(), mode == Mode::Repair);
        assert_eq!(
            result.audit.decision.to_string(),
            if mode == Mode::Strict {
                "blocked"
            } else {
                "repaired"
            }
        );
        assert_eq!(
            engine.verifications.load(Ordering::SeqCst),
            if mode == Mode::Strict { 0 } else { 2 }
        );
    }
    Ok(())
}
