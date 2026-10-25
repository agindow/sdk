#[path = "support/activation_audit.rs"]
mod activation;
#[path = "support/cross_endpoint.rs"]
mod cross;
#[path = "support/invariants.rs"]
mod local;
#[path = "support/statistical.rs"]
mod stats;
use activation::Error;
use agw_diff::{Change, DriftClass, Kind, Reason, Report, Side};
use agw_guard::statistical::{Decision, Engine, Operation, Plan};
use agw_invariants::{Verdict, activation::Stage};

#[test]
fn unavailable_invariants_preserve_aggregate_blocked_and_degraded_status() -> Result<(), Error> {
    let engine = Engine::statistical(
        Plan::new(local::identity(), vec![])?
            .with_invariants(Side::Request, local::selection()?)?,
    );
    let report = Report {
        changes: vec![Change {
            side: Side::Request,
            path: "/other".into(),
            to_path: None,
            kind: Kind::EnumExpanded,
            class: DriftClass::Semantic,
            reason: Reason::EnumExpanded,
        }],
        skipped: None,
    };
    for (operation, decision, status) in [
        (Operation::Write, Decision::Halt, "blocked"),
        (Operation::Read, Decision::Degraded, "degraded"),
    ] {
        let mut body = stats::body(b"{}");
        body.findings = Some(&report);
        let output = engine
            .begin_call(
                stats::context("repair")?,
                operation,
                "audit",
                100_000_000,
                0,
            )
            .dispatch(body, None, || (), |_| stats::body(b"{}"));
        assert_eq!(output.request.decision, decision);
        assert_eq!(output.request.unavailable_invariants, 6);
        let audit = output.request.invariant_audit.ok_or("audit")?;
        assert_eq!(audit.decision.to_string(), status);
        assert_eq!(output.request.headers.get("X-Agindow-Status"), Some(status));
        assert_eq!(output.dispatched, decision != Decision::Halt);
        assert_eq!(output.bypasses, 0);
    }
    Ok(())
}

#[test]
fn unavailable_only_invariants_keep_bypassed_audit_and_warning() -> Result<(), Error> {
    let output = local::engine()?
        .begin_call(
            stats::context("repair")?,
            Operation::Write,
            "audit",
            100_000_000,
            0,
        )
        .dispatch(stats::body(b"{}"), None, || (), |_| stats::body(b"{}"));
    assert_eq!(output.request.decision, Decision::Pass);
    assert_eq!(
        output.request.headers.get("X-Agindow-Status"),
        Some("bypassed")
    );
    assert!(output.request.warning.is_some());
    assert_eq!(
        output
            .request
            .invariant_audit
            .ok_or("audit")?
            .decision
            .to_string(),
        "bypassed"
    );
    assert!(output.dispatched);
    assert_eq!(output.bypasses, 0);
    Ok(())
}

#[test]
fn opposite_phase_paired_findings_retain_full_activation_provenance() -> Result<(), Error> {
    let (controller, _) = activation::controller(true)?;
    let mut engine = Engine::statistical(Plan::new(local::identity(), vec![])?);
    engine.publish_activation(Side::Request, controller.active().ok_or("generation")?)?;
    let bytes: &[u8] = br#"{"amount":102}"#;
    let output = engine
        .begin_call(
            stats::context("repair")?,
            Operation::Write,
            "audit",
            100_000_000,
            0,
        )
        .with_workflow(Ok(cross::snapshot(Some(100), 8000)?), 2)
        .dispatch(stats::body(b"{}"), None, || bytes, |b| stats::body(b));
    assert!(std::ptr::eq(output.outcome.ok_or("outcome")?, bytes));
    let response = output.response.ok_or("response")?;
    assert_eq!(response.decision, Decision::Pass);
    assert_eq!(response.invariants[0].verdict, Verdict::Violation);
    let audit = response.invariant_audit.ok_or("audit")?;
    let paired = &audit.metadata["agindow_cross_endpoint"]["activation"];
    assert_eq!(paired["generation"], 1);
    assert_eq!(paired["stage"], "Shadow");
    assert_eq!(paired["authorization"]["expires"], 100);
    assert_eq!(paired["receipts"][0]["holdout"], "holdout-1");
    assert_eq!(paired["receipts"][0]["valid"], 10);
    assert_eq!(paired["receipts"][0]["false_positives"], 0);
    assert_eq!(audit.metadata.get("agindow_activation"), Some(paired));
    assert!(!audit.proof.write_integrity);
    Ok(())
}

#[test]
fn mixed_local_and_paired_generations_keep_separate_qualification_evidence() -> Result<(), Error> {
    let (mut controller, candidate) = activation::controller(false)?;
    activation::publish(&mut controller, &candidate, Stage::Reads)?;
    let (paired, _) = activation::controller(true)?;
    let mut engine = Engine::statistical(Plan::new(local::identity(), vec![])?);
    engine.publish_activation(
        Side::Response,
        controller.active().ok_or("local generation")?,
    )?;
    engine.publish_activation(Side::Request, paired.active().ok_or("paired generation")?)?;
    let output = engine
        .begin_call(
            stats::context("repair")?,
            Operation::Read,
            "audit",
            100_000_000,
            0,
        )
        .with_workflow(Ok(cross::snapshot(Some(100), 8000)?), 2)
        .dispatch(
            stats::body(b"{}"),
            None,
            || (),
            |_| stats::body(br#"{"amount":102,"n":11}"#),
        );
    let response = output.response.ok_or("response")?;
    assert_eq!(response.decision, Decision::Degraded);
    assert_eq!(response.invariants.len(), 2);
    let audit = response.invariant_audit.ok_or("audit")?;
    let local = &audit.metadata["agindow_activation"];
    let paired = &audit.metadata["agindow_cross_endpoint"]["activation"];
    assert_eq!(local["generation"], 2);
    assert_eq!(local["stage"], "Reads");
    assert_eq!(local["receipts"][1]["holdout"], "holdout-2");
    assert_eq!(paired["generation"], 1);
    assert_eq!(paired["stage"], "Shadow");
    assert_eq!(paired["authorization"]["expires"], 100);
    assert_eq!(paired["receipts"][0]["holdout"], "holdout-1");
    assert!(audit.patch.ok_or("patch")?.byte_identical);
    Ok(())
}
