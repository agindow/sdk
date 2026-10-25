#[path = "support/statistical.rs"]
mod support;
use agw_diff::Side;
use agw_guard::statistical::{Decision, Engine, Operation, Plan};

#[test]
fn suspended_call_freezes_plan_mode_and_shares_the_two_phase_budget()
-> Result<(), Box<dyn std::error::Error>> {
    let mut engine = Engine::statistical(Plan::new(
        support::identity(),
        vec![support::field(Side::Response, "1", "100")?],
    )?);
    let call = engine.begin_call(
        support::context("repair")?,
        Operation::Write,
        "audit",
        100_000_000,
        0,
    );
    let session = call.start(support::body(b"{}"));
    assert_eq!(session.request().decision, Decision::Pass);
    engine.replace(Plan::new(support::identity(), vec![])?);
    let output = session.finish(support::body(b"{\"value\":100}"));
    assert_eq!(output.response.ok_or("response")?.decision, Decision::Halt);
    assert_eq!(output.mode, agw_guard::config::Mode::Repair);
    assert_eq!(output.bypasses, 0);
    let session = engine
        .begin_call(support::context("strict")?, Operation::Write, "audit", 0, 0)
        .start(support::body(b"{}"));
    let output = session.finish(support::body(b"{}"));
    assert_eq!(output.bypasses, 1);
    assert!(output.response.is_none());
    Ok(())
}
#[test]
fn unavailable_host_capture_is_one_bypass_and_counts_toward_the_breaker()
-> Result<(), Box<dyn std::error::Error>> {
    use agw_guard::availability::EngineFault;
    let engine = Engine::statistical(Plan::new(
        support::identity(),
        vec![support::field(Side::Response, "1", "1")?],
    )?);
    for _ in 0..3 {
        let session = engine
            .begin_call(
                support::context("strict")?,
                Operation::Write,
                "audit",
                100_000_000,
                0,
            )
            .start(support::body(b"{}"));
        let outcome = session.finish_unavailable(support::body(b"{}"));
        assert!(outcome.dispatched);
        assert_eq!(outcome.bypasses, 1);
        assert_eq!(
            outcome.response.ok_or("response")?.fault,
            Some(EngineFault::ResourceExhausted)
        );
    }
    let call = engine
        .begin_call(
            support::context("strict")?,
            Operation::Write,
            "audit",
            100_000_000,
            0,
        )
        .start(support::body(b"{}"));
    assert_eq!(call.request().fault, Some(EngineFault::CircuitOpen));
    Ok(())
}
