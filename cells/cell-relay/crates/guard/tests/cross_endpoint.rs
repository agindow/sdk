#[path = "support/cross_endpoint.rs"]
mod fixture;
#[path = "support/invariants.rs"]
mod local;
#[path = "support/statistical.rs"]
mod stats;
use agw_guard::statistical::{Decision, Operation};
use agw_invariants::{
    Verdict,
    cross_endpoint::{Limits, Reason, Store},
};
use fixture::Error;
#[test]
fn current_endpoint_is_compared_before_write_with_correlated_audit_provenance() -> Result<(), Error>
{
    let bytes: &[u8] = br#"{ "amount":102 }"#;
    let output = fixture::engine()?
        .begin_call(
            stats::context("repair")?,
            Operation::Write,
            "audit",
            100_000_000,
            0,
        )
        .with_workflow(Ok(fixture::snapshot(Some(100), 8000)?), 2)
        .dispatch(
            stats::body(bytes),
            None,
            || panic!("divergent write must not dispatch"),
            |_| stats::body(bytes),
        );
    assert_eq!(output.request.decision, Decision::Halt);
    assert!(
        output.request.cross_observations.is_empty(),
        "blocked requests must not publish observations"
    );
    assert_eq!(output.request.cross_endpoint[0].reason, Reason::Divergence);
    assert_eq!(output.request.invariants[0].verdict, Verdict::Violation);
    assert_eq!(output.bypasses, 0);
    let audit = serde_json::to_value(output.request.invariant_audit.ok_or("audit")?)?;
    assert_eq!(audit["drift"]["class"], "semantic");
    assert_eq!(
        audit["metadata"]["agindow_cross_endpoint"]["records"][0]["left_source"]["operation"],
        "charge"
    );
    assert_eq!(
        audit["metadata"]["agindow_cross_endpoint"]["scope"]["entity"],
        "charge-1"
    );
    assert_eq!(
        audit["metadata"]["agindow_cross_endpoint"]["records"][0]["left"]["value"],
        100
    );
    assert_eq!(
        audit["metadata"]["agindow_cross_endpoint"]["records"][0]["right"]["value"],
        102
    );
    assert!(!audit["proof"]["write_integrity"].as_bool().ok_or("proof")?);
    Ok(())
}
#[test]
fn partial_stale_unsupported_and_out_of_order_evidence_never_fabricate_findings()
-> Result<(), Error> {
    let store = Store::new(Limits::default())?;
    for (snapshot, bytes, reason) in [
        (
            store.snapshot(&fixture::scope())?,
            br#"{"amount":102}"#.as_slice(),
            Reason::Partial,
        ),
        (
            fixture::snapshot(Some(100), -200_001)?,
            br#"{"amount":102}"#.as_slice(),
            Reason::Stale,
        ),
        (
            fixture::snapshot(None, 8000)?,
            br#"{"amount":102}"#.as_slice(),
            Reason::Unsupported,
        ),
        (
            fixture::snapshot(Some(100), 8000)?,
            br#"{"amount":1e2}"#.as_slice(),
            Reason::Unsupported,
        ),
    ] {
        let output = fixture::engine()?
            .begin_call(
                stats::context("repair")?,
                Operation::Write,
                "audit",
                100_000_000,
                0,
            )
            .with_workflow(Ok(snapshot), 2)
            .dispatch(stats::body(bytes), None, || bytes, |b| stats::body(b));
        assert_eq!(output.request.cross_endpoint[0].reason, reason);
        assert_eq!(output.request.invariants[0].verdict, Verdict::Unavailable);
        assert!(std::ptr::eq(output.outcome.ok_or("outcome")?, bytes));
        assert!(
            output
                .request
                .invariant_audit
                .ok_or("audit")?
                .drift
                .is_none()
        );
    }
    Ok(())
}
#[test]
fn observations_are_returned_for_explicit_publication_without_mutating_snapshot()
-> Result<(), Error> {
    let store = Store::new(Limits::default())?;
    store.record(&[fixture::observation(Some(100), 8000)])?;
    let frozen = store.snapshot(&fixture::scope())?;
    let bytes: &[u8] = br#"{"amount":101}"#;
    let output = fixture::engine()?
        .begin_call(
            stats::context("repair")?,
            Operation::Write,
            "audit",
            100_000_000,
            0,
        )
        .with_workflow(Ok(frozen.clone()), 2)
        .dispatch(stats::body(bytes), None, || bytes, |b| stats::body(b));
    assert_eq!(output.request.cross_endpoint[0].reason, Reason::Pass);
    assert_eq!(frozen.observations().len(), 1);
    assert_eq!(output.request.cross_observations.len(), 1);
    store.record(&output.request.cross_observations)?;
    assert_eq!(store.snapshot(&fixture::scope())?.observations().len(), 2);
    assert_eq!(frozen.observations().len(), 1);
    Ok(())
}
#[test]
fn mixed_local_invariant_and_cross_endpoint_findings_survive_policy() -> Result<(), Error> {
    let bytes: &[u8] = br#"{"amount":102,"tax":20,"total":118}"#;
    let output = agw_guard::statistical::Engine::statistical(
        agw_guard::statistical::Plan::new(stats::identity(), vec![])?
            .with_cross_endpoint(fixture::selection()?)?
            .with_invariants(agw_diff::Side::Request, local::selection()?)?,
    )
    .begin_call(
        stats::context("repair")?,
        Operation::Write,
        "audit",
        100_000_000,
        0,
    )
    .with_workflow(Ok(fixture::snapshot(Some(100), 8000)?), 2)
    .dispatch(
        stats::body(bytes),
        None,
        || panic!("both semantic findings must halt"),
        |_| stats::body(bytes),
    );
    assert_eq!(output.request.invariants.len(), 7);
    assert_eq!(
        output
            .request
            .invariants
            .iter()
            .filter(|r| r.verdict == Verdict::Violation)
            .count(),
        2
    );
    assert_eq!(output.request.decision, Decision::Halt);
    Ok(())
}
