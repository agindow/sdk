#[path = "support/cross_endpoint.rs"]
mod cross;
#[path = "../../invariants/tests/support/activation.rs"]
mod fixture;
#[path = "support/statistical.rs"]
mod stats;
use agw_diff::Side;
use agw_guard::statistical::{Decision, Engine, Identity, Operation, Plan};
use agw_invariants::activation::*;
type TestError = Box<dyn std::error::Error>;
#[test]
fn paired_candidates_keep_identity_attestation_controls_and_shadow_policy() -> Result<(), TestError>
{
    let mut candidate = fixture::candidate();
    candidate.scope.tenant = "tenant".into();
    candidate.scope.connection = "conn".into();
    candidate.scope.vendor = "vendor".into();
    candidate.scope.operation = "endpoint".into();
    candidate.scope.contract = 7;
    candidate.rules.clear();
    candidate.bindings.clear();
    candidate.peers = vec![cross::rule()];
    let mut policy = fixture::policy();
    policy.scope = candidate.scope.clone();
    policy.families = vec![Family::CrossEndpoint];
    let mut c = Controller::new(candidate.scope.clone(), 32)?;
    c.configure(Some(policy.clone()), Default::default())?;
    let mut receipt = fixture::receipt(1, Stage::Shadow);
    receipt.scope = candidate.scope.clone();
    c.record(receipt)?;
    assert_eq!(
        c.prepare(candidate.clone(), Stage::Shadow, 10)?
            .decision()
            .reason,
        Reason::Attestation
    );
    policy.revision = 2;
    policy.peer_attestations = vec![PeerAttestation {
        rule: cross::rule(),
        revision_semantics: "same charge immutable revision".into(),
    }];
    c.configure(Some(policy), Default::default())?;
    let mut receipt = fixture::receipt(2, Stage::Shadow);
    receipt.scope = candidate.scope.clone();
    receipt.policy = 2;
    c.record(receipt)?;
    let identity = Identity {
        tenant: "tenant".into(),
        connection: "conn".into(),
        vendor: "vendor".into(),
        operation: "endpoint".into(),
        segment: "s".into(),
    };
    let mut e = Engine::statistical(Plan::new(identity, vec![])?);
    let pending = c.prepare(candidate.clone(), Stage::Shadow, 10)?;
    c.publish_with(pending, 10, |g| {
        e.publish_activation(Side::Request, g)
            .map_err(|_| Error::Compilation)
    })?;
    let held = e
        .begin_call(
            stats::context("repair")?,
            Operation::Write,
            "held",
            100_000_000,
            0,
        )
        .with_workflow(Ok(cross::snapshot(Some(100), 8000)?), 2);
    let output = held.dispatch(
        stats::body(br#"{"amount":102}"#),
        None,
        || (),
        |_| stats::body(b"{}"),
    );
    assert_eq!(output.request.decision, Decision::Pass);
    assert_eq!(
        output.request.cross_endpoint[0].reason,
        agw_invariants::cross_endpoint::Reason::Divergence
    );
    c.configure(
        None,
        agw_invariants::store::Controls {
            disabled: vec!["gross-agrees".into()],
            ..Default::default()
        },
    )?;
    c.reconcile_with(|g| {
        e.publish_activation(Side::Request, g)
            .map_err(|_| Error::Compilation)
    })?;
    let output = e
        .begin_call(
            stats::context("strict")?,
            Operation::Write,
            "a",
            100_000_000,
            0,
        )
        .with_workflow(Ok(cross::snapshot(Some(100), 8000)?), 2)
        .dispatch(
            stats::body(br#"{"amount":102}"#),
            None,
            || (),
            |_| stats::body(b"{}"),
        );
    assert!(output.request.cross_endpoint.is_empty());
    Ok(())
}

#[test]
fn replacing_a_shadow_set_removes_old_peers_and_can_publish_both_sides() -> Result<(), TestError> {
    let mut candidate = fixture::candidate();
    candidate.rules.clear();
    let mut peer = cross::rule();
    peer.right.operation = "o".into();
    candidate.peers = vec![peer.clone()];
    let mut policy = fixture::policy();
    policy.families.push(Family::CrossEndpoint);
    policy.peer_attestations = vec![PeerAttestation {
        rule: peer,
        revision_semantics: "immutable entity revision".into(),
    }];
    let mut c = Controller::new(fixture::scope(), 32)?;
    c.configure(Some(policy), Default::default())?;
    c.record(fixture::receipt(1, Stage::Shadow))?;
    c.publish_prepared(candidate, Stage::Shadow, 10)?;
    let identity = Identity {
        tenant: "t".into(),
        connection: "c".into(),
        vendor: "v".into(),
        operation: "o".into(),
        segment: "s".into(),
    };
    let mut e = Engine::statistical(Plan::new(identity, vec![])?);
    let generation = c.active().ok_or("generation")?;
    e.publish_activation(Side::Request, generation.clone())?;
    e.publish_activation(Side::Response, generation)?;
    let mut next = fixture::candidate();
    next.version = 2;
    let mut r = fixture::receipt(2, Stage::Shadow);
    r.candidate = 2;
    c.record(r)?;
    let pending = c.prepare(next, Stage::Shadow, 20)?;
    c.publish_with(pending, 20, |g| {
        e.publish_activation(Side::Request, g)
            .map_err(|_| Error::Compilation)
    })?;
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
            || (),
            |_| stats::body(b"{}"),
        );
    assert!(output.request.cross_endpoint.is_empty());
    Ok(())
}
