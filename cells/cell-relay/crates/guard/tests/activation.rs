#[path = "../../invariants/tests/support/activation.rs"]
mod fixture;
#[path = "support/statistical.rs"]
mod stats;
use agw_diff::Side;
use agw_guard::{
    config::Mode,
    statistical::{Decision, Engine, Identity, Operation, Plan},
};
use agw_invariants::activation::*;
type TestError = Box<dyn std::error::Error>;
fn identity() -> Identity {
    Identity {
        tenant: "t".into(),
        connection: "c".into(),
        vendor: "v".into(),
        operation: "o".into(),
        segment: "s".into(),
    }
}
#[test]
fn shadow_and_reads_cannot_enable_blocking_writes_or_change_call_modes() -> Result<(), TestError> {
    let mut controller = fixture::authorized()?;
    controller.record(fixture::receipt(1, Stage::Shadow))?;
    controller.publish_prepared(fixture::candidate(), Stage::Shadow, 10)?;
    let mut engine = Engine::statistical(Plan::new(identity(), vec![])?);
    engine.publish_activation(Side::Request, controller.active().ok_or("active")?)?;
    let held = engine.begin_call(
        stats::context("strict")?,
        Operation::Read,
        "held",
        100_000_000,
        0,
    );
    for stage in [Stage::Shadow, Stage::Reads] {
        if stage == Stage::Reads {
            controller.record(fixture::receipt(2, stage))?;
            controller.publish_prepared(fixture::candidate(), stage, 20)?;
            engine.publish_activation(Side::Request, controller.active().ok_or("active")?)?;
        }
        for operation in [Operation::Read, Operation::Write, Operation::Unknown] {
            let bytes: &[u8] = br#"{ "n": 11 }"#;
            let output = engine
                .begin_call(stats::context("strict")?, operation, "a", 100_000_000, 0)
                .dispatch(stats::body(bytes), None, || bytes, |b| stats::body(b));
            assert_eq!(output.mode, Mode::Strict);
            let halt = stage == Stage::Reads && operation == Operation::Read;
            assert_eq!(
                output.request.decision,
                if halt { Decision::Halt } else { Decision::Pass }
            );
            assert_eq!(output.dispatched, !halt);
            if !halt {
                assert!(std::ptr::eq(output.outcome.ok_or("outcome")?, bytes));
            }
            let audit = output.request.invariant_audit.ok_or("audit")?;
            assert!(!audit.proof.write_integrity);
            assert!(audit.metadata.contains_key("agindow_activation"));
        }
    }
    let output = held.dispatch(
        stats::body(br#"{"n":11}"#),
        None,
        || (),
        |_| stats::body(b"{}"),
    );
    assert_eq!(output.request.decision, Decision::Pass);
    Ok(())
}
#[test]
fn default_and_customer_selected_plans_cannot_be_overwritten() -> Result<(), TestError> {
    let mut controller = fixture::authorized()?;
    controller.record(fixture::receipt(1, Stage::Shadow))?;
    controller.publish_prepared(fixture::candidate(), Stage::Shadow, 10)?;
    let g = controller.active().ok_or("active")?;
    assert!(
        Engine::default()
            .publish_activation(Side::Request, g.clone())
            .is_err()
    );
    let mut engine = Engine::statistical(Plan::new(identity(), vec![])?);
    engine.publish_activation(Side::Request, g.clone())?;
    assert!(engine.publish_activation(Side::Request, g).is_err());
    Ok(())
}

#[test]
fn external_enum_evidence_is_not_suppressed_by_same_named_shadow_rule() -> Result<(), TestError> {
    let mut c = fixture::authorized()?;
    c.record(fixture::receipt(1, Stage::Shadow))?;
    c.publish_prepared(fixture::candidate(), Stage::Shadow, 10)?;
    let mut e = Engine::statistical(Plan::new(identity(), vec![])?);
    e.publish_activation(Side::Request, c.active().ok_or("active")?)?;
    let report = agw_diff::Report {
        changes: vec![agw_diff::Change {
            side: Side::Request,
            path: "range".into(),
            to_path: None,
            kind: agw_diff::Kind::EnumExpanded,
            class: agw_diff::DriftClass::Semantic,
            reason: agw_diff::Reason::EnumExpanded,
        }],
        ..agw_diff::Report::default()
    };
    let mut body = stats::body(br#"{"n":1}"#);
    body.findings = Some(&report);
    let output = e
        .begin_call(
            stats::context("repair")?,
            Operation::Write,
            "a",
            100_000_000,
            0,
        )
        .dispatch(body, None, || (), |_| stats::body(b"{}"));
    assert_eq!(output.request.decision, Decision::Halt);
    Ok(())
}

#[test]
fn scoped_mode_permissions_and_publisher_failures_preserve_selected_plan() -> Result<(), TestError>
{
    let mut c = Controller::new(fixture::scope(), 32)?;
    let mut p = fixture::policy();
    p.modes = vec![RolloutMode::Repair];
    c.configure(Some(p), Default::default())?;
    let mut e = Engine::statistical(Plan::new(identity(), vec![])?);
    for (id, stage, at) in [(1, Stage::Shadow, 10), (2, Stage::Reads, 20)] {
        c.record(fixture::receipt(id, stage))?;
        let pending = c.prepare(fixture::candidate(), stage, at)?;
        c.publish_with(pending, at, |g| {
            e.publish_activation(Side::Request, g)
                .map_err(|_| Error::Compilation)
        })?;
    }
    let output = e
        .begin_call(
            stats::context("strict")?,
            Operation::Read,
            "a",
            100_000_000,
            0,
        )
        .dispatch(
            stats::body(br#"{"n":11}"#),
            None,
            || (),
            |_| stats::body(b"{}"),
        );
    assert_eq!(output.mode, Mode::Strict);
    assert_eq!(output.request.decision, Decision::Pass);
    let output = e
        .begin_call(
            stats::context("repair")?,
            Operation::Read,
            "a",
            100_000_000,
            0,
        )
        .dispatch(
            stats::body(br#"{"n":11}"#),
            None,
            || (),
            |_| stats::body(b"{}"),
        );
    assert_eq!(output.request.decision, Decision::Degraded);
    // A controller edit cannot overwrite an explicit customer selection.
    let mut explicit = Plan::new(identity(), vec![])?;
    explicit = explicit.with_invariants(
        Side::Request,
        agw_guard::invariant::Selection::new(c.active().ok_or("active")?.snapshot().clone())?,
    )?;
    let mut customer = Engine::statistical(explicit);
    assert!(
        customer
            .publish_activation(Side::Request, c.active().ok_or("active")?)
            .is_err()
    );
    Ok(())
}

#[test]
fn calibrated_response_halts_retain_the_original_attempt_and_warming_bypasses_once()
-> Result<(), TestError> {
    let mut p = fixture::policy();
    p.calibration = Some(Calibration {
        dataset: "modeled-test-host-attestation".into(),
        candidate: 1,
        sampling: "independent comparable calls".into(),
        independent_production: true,
        counts: fixture::receipt(1, Stage::Shadow).counts,
    });
    let mut c = Controller::new(fixture::scope(), 32)?;
    c.configure(Some(p), Default::default())?;
    for (id, stage, at) in [
        (1, Stage::Shadow, 10),
        (2, Stage::Reads, 20),
        (3, Stage::Blocking, 30),
    ] {
        let mut r = fixture::receipt(id, stage);
        if stage == Stage::Blocking {
            r.started = 21;
            r.finished = 25;
        }
        c.record(r)?;
        c.publish_prepared(fixture::candidate(), stage, at)?;
    }
    let mut e = Engine::statistical(Plan::new(identity(), vec![])?);
    e.publish_activation(Side::Response, c.active().ok_or("active")?)?;
    let original: &[u8] = br#"{ "n":11 }"#;
    let output = e
        .begin_call(
            stats::context("repair")?,
            Operation::Write,
            "a",
            100_000_000,
            0,
        )
        .dispatch(
            stats::body(br#"{"n":1}"#),
            None,
            || original,
            |b| stats::body(b),
        );
    assert_eq!(
        output.response.as_ref().ok_or("response")?.decision,
        Decision::Halt
    );
    assert!(output.dispatched);
    assert!(std::ptr::eq(output.outcome.ok_or("outcome")?, original));
    e.set_ready(false);
    let output = e
        .begin_call(
            stats::context("repair")?,
            Operation::Write,
            "a",
            100_000_000,
            0,
        )
        .dispatch(
            stats::body(br#"{"n":11}"#),
            None,
            || original,
            |b| stats::body(b),
        );
    assert_eq!(output.bypasses, 1);
    assert!(std::ptr::eq(output.outcome.ok_or("outcome")?, original));
    Ok(())
}
