#[path = "support/gated.rs"]
mod support;
use agw_diff::{DriftClass as Class, Side};
use agw_guard::{
    config::{Config, RuntimeConfig},
    gated::{Body, Call},
    readiness::AvailabilityGate,
};
use agw_policy::{Decision, Operation};
use support::{Engine, draft};
type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn control_receipt_requires_actor_and_keeps_frozen_call_configuration() -> TestResult {
    let runtime = RuntimeConfig::from_sources(Config::default(), None)?;
    let before = runtime.begin_call(None)?;
    assert!(before.fail_closed());
    let ts = draft()?.ts;
    assert!(runtime.reload_fail_closed(false, " ", ts).is_err());
    assert!(runtime.begin_call(None)?.fail_closed());
    let receipt = runtime.reload_fail_closed(false, "owner@example.com", ts)?;
    assert!(!receipt.fail_closed);
    assert_eq!(receipt.actor, "owner@example.com");
    assert_eq!(receipt.ts, ts);
    assert!(
        receipt
            .warning
            .is_some_and(|w| w.contains("200") && w.contains("wrong data"))
    );
    let during = runtime.begin_call(None)?;
    runtime.reload_mode("strict")?;
    assert!(!during.fail_closed());
    assert!(before.fail_closed());
    assert!(
        runtime
            .reload_fail_closed(false, "owner", ts)?
            .warning
            .is_some()
    );
    assert!(
        runtime
            .reload_fail_closed(true, "owner", ts)?
            .warning
            .is_none()
    );
    assert!(runtime.begin_call(None)?.fail_closed());
    Ok(())
}

#[test]
fn opt_out_dispatches_unknown_but_cannot_open_semantic_or_strict() -> TestResult {
    for mode in ["observe", "repair", "strict"] {
        for class in [Class::Shape, Class::Unknown, Class::Semantic] {
            let runtime = RuntimeConfig::from_sources(Config::default(), None)?;
            runtime.reload_mode(mode)?;
            runtime.reload_fail_closed(false, "owner", draft()?.ts)?;
            let engine = Engine::new(Some(class));
            let gate = AvailabilityGate::new(true);
            let mut audit = draft()?;
            audit.operation = Operation::Write;
            let ts = audit.ts;
            let bytes = b"{ \"old\": 1.00 }";
            let mut calls = 0;
            let result = Call::new(runtime.begin_call(None)?, &gate, 100_000).execute(
                Body::complete(bytes),
                &engine,
                audit,
                |sent| {
                    calls += 1;
                    assert!(std::ptr::eq(sent, bytes.as_slice()));
                    assert!(runtime.reload_fail_closed(true, "owner", ts).is_ok());
                    (77, Body::complete(bytes))
                },
            );
            let halted = mode == "strict" || (mode == "repair" && class == Class::Semantic);
            assert_eq!(
                result.audit.metadata["agindow_policy"]["fail_closed"],
                false
            );
            assert_eq!(calls, usize::from(!halted));
            assert_eq!(result.request.decision == Decision::Halt, halted);
            if !halted {
                let response = result.response.ok_or("missing response")?;
                assert_ne!(response.decision, Decision::Halt);
                assert!(std::ptr::eq(response.body.as_ref(), bytes.as_slice()));
                assert_eq!(result.outcome, Some(77));
            }
        }
    }
    Ok(())
}

#[test]
fn protection_does_not_change_availability_bypass() -> TestResult {
    for ready in [false, true] {
        let runtime = RuntimeConfig::from_sources(Config::default(), None)?;
        runtime.reload_mode("strict")?;
        runtime.reload_fail_closed(false, "owner", draft()?.ts)?;
        let mut engine = Engine::new(Some(Class::Semantic));
        engine.fault = Some(Side::Request);
        let gate = AvailabilityGate::new(ready);
        let result = Call::new(runtime.begin_call(None)?, &gate, 100_000).execute(
            Body::complete(b"{}"),
            &engine,
            draft()?,
            |_| (19, Body::complete(b"{}")),
        );
        assert_eq!(result.outcome, Some(19));
        assert_eq!(result.bypasses, 1);
    }
    Ok(())
}
