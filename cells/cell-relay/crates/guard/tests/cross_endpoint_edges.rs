#[path = "support/cross_endpoint.rs"]
mod fixture;
#[path = "support/statistical.rs"]
mod stats;
use agw_guard::{
    availability::EngineFault,
    cross_endpoint::Selection,
    statistical::{Decision, Engine, Operation, Plan},
};
use agw_invariants::cross_endpoint::{Error as StoreError, Limits, Reason, Side, Store};
use fixture::Error;
#[test]
fn scope_faults_contention_and_budget_fail_open_once_with_original_identity() -> Result<(), Error> {
    for state in 0..5 {
        let store = Store::new(Limits::default())?;
        let mut o = fixture::observation(Some(100), 8000);
        o.scope.tenant = "another-tenant".into();
        store.record(std::slice::from_ref(&o))?;
        let view = if state == 0 {
            store.snapshot(&o.scope)
        } else if state == 1 {
            Err(StoreError::Busy)
        } else if state == 2 {
            Err(StoreError::Poisoned)
        } else {
            Ok(fixture::snapshot(Some(100), 8000)?)
        };
        let original = Box::new("upstream error");
        let ptr = &*original as *const &str;
        let output = fixture::engine()?
            .begin_call(
                stats::context("strict")?,
                Operation::Write,
                "audit",
                if state == 3 { 0 } else { 100_000_000 },
                0,
            )
            .with_workflow(view, if state == 4 { 0 } else { 2 })
            .execute(stats::body(br#"{"amount":999}"#), None, || {
                (Err::<(), _>(original), stats::body(b"{}"))
            });
        assert_eq!(output.bypasses, 1);
        assert!(output.response.is_none());
        assert!(output.request.cross_endpoint.is_empty());
        assert!(output.request.invariant_audit.is_none());
        assert_eq!(
            output.request.fault,
            Some(if state == 3 {
                EngineFault::ResourceExhausted
            } else if state == 1 || state == 2 {
                EngineFault::RegistryUnavailable
            } else {
                EngineFault::RegistryCorrupt
            })
        );
        let kept = output.outcome.ok_or("outcome")?.err().ok_or("error")?;
        assert_eq!(&*kept as *const &str, ptr);
    }
    Ok(())
}
#[test]
fn cached_current_values_cannot_override_new_unsupported_or_out_of_order_calls() -> Result<(), Error>
{
    let store = Store::new(Limits::default())?;
    let mut current = fixture::observation(Some(100), 8000);
    current.key.endpoint = fixture::endpoint("endpoint", Side::Request);
    current.sequence = 3;
    store.record(&[fixture::observation(Some(100), 8000), current])?;
    for (sequence, bytes, reason, observations) in [
        (2, br#"{"amount":100}"#.as_slice(), Reason::OutOfOrder, 0),
        (4, br#"{"amount":1e2}"#.as_slice(), Reason::Unsupported, 1),
    ] {
        let output = fixture::engine()?
            .begin_call(
                stats::context("repair")?,
                Operation::Write,
                "audit",
                100_000_000,
                0,
            )
            .with_workflow(Ok(store.snapshot(&fixture::scope())?), sequence)
            .dispatch(stats::body(bytes), None, || bytes, |b| stats::body(b));
        assert_eq!(output.request.cross_endpoint[0].reason, reason);
        assert_eq!(output.request.cross_observations.len(), observations);
        assert_eq!(output.request.decision, Decision::Pass);
        if observations > 0 {
            assert_eq!(output.request.cross_observations[0].value, None);
        }
    }
    Ok(())
}
#[test]
fn both_endpoint_directions_and_response_halts_preserve_completed_attempt() -> Result<(), Error> {
    let mut identity = stats::identity();
    identity.operation = "charge".into();
    let engine = Engine::statistical(
        Plan::new(identity, vec![])?.with_cross_endpoint(fixture::selection()?)?,
    );
    let store = Store::new(Limits::default())?;
    let mut peer = fixture::observation(Some(100), 8000);
    peer.key.endpoint = fixture::endpoint("endpoint", Side::Request);
    store.record(&[peer])?;
    let bytes: &[u8] = br#"{"amount":102}"#;
    let output = engine
        .begin_call(
            stats::context("repair")?,
            Operation::Write,
            "audit",
            100_000_000,
            0,
        )
        .with_workflow(Ok(store.snapshot(&fixture::scope())?), 2)
        .dispatch(stats::body(b"{}"), None, || bytes, |b| stats::body(b));
    assert!(output.dispatched);
    assert!(std::ptr::eq(output.outcome.ok_or("outcome")?, bytes));
    assert!(output.request.cross_endpoint.is_empty());
    let response = output.response.ok_or("response")?;
    assert_eq!(response.cross_endpoint[0].reason, Reason::Divergence);
    assert_eq!(response.decision, Decision::Halt);
    assert_eq!(response.cross_observations.len(), 1);
    Ok(())
}
#[test]
fn mode_matrix_preserves_observe_and_read_behavior() -> Result<(), Error> {
    for mode in ["observe", "repair", "strict"] {
        for operation in [Operation::Read, Operation::Write, Operation::Unknown] {
            let expected = if mode == "observe" {
                Decision::Pass
            } else if mode == "repair" && operation == Operation::Read {
                Decision::Degraded
            } else {
                Decision::Halt
            };
            let bytes: &[u8] = br#"{"amount":102}"#;
            let calls = std::cell::Cell::new(0);
            let output = fixture::engine()?
                .begin_call(stats::context(mode)?, operation, "audit", 100_000_000, 0)
                .with_workflow(Ok(fixture::snapshot(Some(100), 8000)?), 2)
                .dispatch(
                    stats::body(bytes),
                    None,
                    || {
                        calls.set(calls.get() + 1);
                        bytes
                    },
                    |b| stats::body(b),
                );
            assert_eq!(output.request.decision, expected);
            assert_eq!(calls.get(), usize::from(expected != Decision::Halt));
            assert_eq!(output.bypasses, 0);
            assert!(output.request.warning.is_some());
        }
    }
    Ok(())
}
#[test]
fn invalid_rule_versions_paths_scopes_and_contract_collisions_are_rejected() -> Result<(), Error> {
    assert!(Selection::new(0, 7, vec![fixture::rule()]).is_err());
    let mut rule = fixture::rule();
    rule.max_skew_ms = rule.max_age_ms + 1;
    assert!(Selection::new(1, 7, vec![rule]).is_err());
    let mut rule = fixture::rule();
    rule.left.field = "/bad~2".into();
    assert!(Selection::new(1, 7, vec![rule]).is_err());
    let mut identity = stats::identity();
    identity.operation = "other-operation".into();
    assert!(
        Plan::new(identity, vec![])?
            .with_cross_endpoint(fixture::selection()?)
            .is_err()
    );
    let plan = Plan::new(stats::identity(), vec![])?.with_cross_endpoint(fixture::selection()?)?;
    assert!(plan.with_cross_endpoint(fixture::selection()?).is_err());
    Ok(())
}
