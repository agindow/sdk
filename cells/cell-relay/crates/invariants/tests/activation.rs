#[path = "support/activation.rs"]
mod fixture;
use agw_invariants::activation::*;
use agw_invariants::{Scalar, Verdict, store::Controls};
use fixture::*;

#[test]
fn default_off_and_stages_require_independent_fresh_authority() -> Result<(), Error> {
    let mut c = Controller::new(scope(), 32)?;
    assert_eq!(
        c.prepare(candidate(), Stage::Shadow, 10)?.decision().reason,
        Reason::Unauthorized
    );
    assert!(c.active().is_none());
    c.configure(Some(policy()), Controls::default())?;
    c.record(receipt(1, Stage::Shadow))?;
    let p = c.prepare(candidate(), Stage::Shadow, 10)?;
    assert_eq!(p.decision().reason, Reason::Qualified);
    c.publish(p, 10)?;
    assert_eq!(c.active().ok_or(Error::Invalid)?.stage(), Stage::Shadow);
    assert_eq!(
        c.prepare(candidate(), Stage::Blocking, 30)?
            .decision()
            .reason,
        Reason::StageOrder
    );
    assert_eq!(
        c.prepare(candidate(), Stage::Reads, 20)?.decision().reason,
        Reason::Insufficient
    );
    c.record(receipt(2, Stage::Reads))?;
    c.publish_prepared(candidate(), Stage::Reads, 20)?;
    let mut blocking = receipt(3, Stage::Blocking);
    blocking.started = 21;
    blocking.finished = 25;
    c.record(blocking)?;
    assert_eq!(
        c.prepare(candidate(), Stage::Blocking, 30)?
            .decision()
            .reason,
        Reason::Calibration
    );
    assert_eq!(c.active().ok_or(Error::Invalid)?.stage(), Stage::Reads);
    Ok(())
}

#[test]
fn receipts_cannot_discard_negative_expired_or_contradictory_evidence() -> Result<(), Error> {
    for reason in [
        Reason::Negative,
        Reason::Expired,
        Reason::Incomplete,
        Reason::Contradictory,
    ] {
        let mut c = authorized()?;
        c.record(receipt(1, Stage::Shadow))?;
        let mut bad = receipt(2, Stage::Shadow);
        match reason {
            Reason::Negative => bad.counts.false_positives = 1,
            Reason::Expired => bad.expires = 9,
            Reason::Incomplete => bad.complete = false,
            _ => bad.training = bad.holdout.clone(),
        }
        c.record(bad)?;
        assert_eq!(
            c.prepare(candidate(), Stage::Shadow, 10)?.decision().reason,
            reason
        );
        assert!(c.active().is_none());
    }
    Ok(())
}

#[test]
fn edits_fence_pending_publication_and_retained_generations_are_immutable() -> Result<(), Error> {
    let mut c = authorized()?;
    c.record(receipt(1, Stage::Shadow))?;
    c.publish_prepared(candidate(), Stage::Shadow, 10)?;
    let prior = c.active().ok_or(Error::Invalid)?;
    c.record(receipt(2, Stage::Reads))?;
    let pending = c.prepare(candidate(), Stage::Reads, 20)?;
    c.configure(
        Some(policy()),
        Controls {
            disabled: vec!["range".into()],
            ..Controls::default()
        },
    )?;
    assert_eq!(c.publish(pending, 20), Err(Error::Conflict));
    c.reconcile()?;
    assert!(
        c.active()
            .ok_or(Error::Invalid)?
            .snapshot()
            .program
            .rules()
            .is_empty()
    );
    assert_eq!(
        prior.snapshot().program.rules()[0].evaluate(&[Scalar::Integer(11)]),
        Verdict::Violation
    );
    assert_eq!(
        c.prepare(candidate(), Stage::Reads, 20)?.decision().reason,
        Reason::CustomerControl
    );
    Ok(())
}

#[test]
fn pins_revocation_and_expiry_freeze_without_silent_semantic_demotion() -> Result<(), Error> {
    let mut c = authorized()?;
    c.record(receipt(1, Stage::Shadow))?;
    c.publish_prepared(candidate(), Stage::Shadow, 10)?;
    let old = c.active().ok_or(Error::Invalid)?;
    c.configure(None, Controls::default())?;
    assert_eq!(
        c.prepare(candidate(), Stage::Reads, 20)?.decision().reason,
        Reason::Unauthorized
    );
    assert!(std::sync::Arc::ptr_eq(
        &old,
        &c.active().ok_or(Error::Invalid)?
    ));
    let mut renewed = policy();
    renewed.revision = 2;
    c.configure(
        Some(renewed),
        Controls {
            pin: Some(1),
            ..Controls::default()
        },
    )?;
    let mut upgrade = candidate();
    upgrade.version = 2;
    assert_eq!(
        c.prepare(upgrade, Stage::Shadow, 10)?.decision().reason,
        Reason::CustomerControl
    );
    assert_eq!(
        c.prepare(candidate(), Stage::Reads, 201)?.decision().reason,
        Reason::Expired
    );
    Ok(())
}

#[test]
fn duplicate_receipts_are_idempotent_but_capacity_and_conflicts_fail() -> Result<(), Error> {
    let mut c = Controller::new(scope(), 1)?;
    c.record(receipt(1, Stage::Shadow))?;
    c.record(receipt(1, Stage::Shadow))?;
    let mut changed = receipt(1, Stage::Shadow);
    changed.complete = false;
    assert_eq!(c.record(changed), Err(Error::Conflict));
    assert_eq!(c.record(receipt(2, Stage::Shadow)), Err(Error::Capacity));
    Ok(())
}

#[test]
fn family_attestation_permission_and_exact_coverage_gates() -> Result<(), Error> {
    for reason in [
        Reason::Family,
        Reason::Attestation,
        Reason::Unauthorized,
        Reason::Insufficient,
    ] {
        let mut c = Controller::new(scope(), 32)?;
        let mut p = policy();
        match reason {
            Reason::Family => p.families.clear(),
            Reason::Attestation => p.attestations.clear(),
            Reason::Unauthorized => p.stages = vec![Stage::Reads],
            _ => p.thresholds.minimum_valid = 11,
        }
        c.configure(Some(p), Controls::default())?;
        c.record(receipt(1, Stage::Shadow))?;
        assert_eq!(
            c.prepare(candidate(), Stage::Shadow, 10)?.decision().reason,
            reason
        );
    }
    let mut c = authorized()?;
    let mut r = receipt(1, Stage::Shadow);
    r.counts.eligible = 21;
    c.record(r)?;
    assert_eq!(
        c.prepare(candidate(), Stage::Shadow, 10)?.decision().reason,
        Reason::Insufficient
    );
    Ok(())
}

#[test]
fn scope_definition_and_controller_origins_are_fenced() -> Result<(), Error> {
    let mut c = authorized()?;
    c.record(receipt(1, Stage::Shadow))?;
    let pending = c.prepare(candidate(), Stage::Shadow, 10)?;
    let mut other = authorized()?;
    assert_eq!(other.publish(pending, 20), Err(Error::Conflict));
    let mut changed = candidate();
    changed.bindings = vec!["/other".into()];
    assert!(matches!(
        c.prepare(changed, Stage::Shadow, 10),
        Err(Error::Compilation)
    ));
    let mut changed = candidate();
    changed.rules[0].form = agw_invariants::Form::Range {
        field: "/n".into(),
        min: 0,
        max: 11,
    };
    assert!(matches!(
        c.prepare(changed, Stage::Shadow, 10),
        Err(Error::Conflict)
    ));
    let mut wrong = receipt(2, Stage::Shadow);
    wrong.scope.tenant = "other".into();
    assert_eq!(c.record(wrong), Err(Error::Invalid));
    Ok(())
}

#[test]
fn changed_evidence_invalidates_prepared_work_and_missing_replay_keeps_prior() -> Result<(), Error>
{
    let mut c = authorized()?;
    c.record(receipt(1, Stage::Shadow))?;
    let pending = c.prepare(candidate(), Stage::Shadow, 10)?;
    c.record(receipt(2, Stage::Shadow))?;
    assert_eq!(c.publish(pending, 20), Err(Error::Conflict));
    c.publish_prepared(candidate(), Stage::Shadow, 10)?;
    let prior = c.active().ok_or(Error::Invalid)?;
    assert_eq!(
        c.prepare(candidate(), Stage::Reads, 20)?.decision().reason,
        Reason::Insufficient
    );
    assert!(std::sync::Arc::ptr_eq(
        &prior,
        &c.active().ok_or(Error::Invalid)?
    ));
    Ok(())
}

#[test]
fn publisher_failure_preserves_generation_and_is_recorded() -> Result<(), Error> {
    let mut c = authorized()?;
    c.record(receipt(1, Stage::Shadow))?;
    let pending = c.prepare(candidate(), Stage::Shadow, 10)?;
    assert_eq!(
        c.publish_with(pending, 10, |_| Err(Error::Compilation)),
        Err(Error::Compilation)
    );
    assert!(c.active().is_none());
    assert_eq!(
        c.decisions().last().ok_or(Error::Invalid)?.transition,
        Transition::PublicationFailed
    );
    Ok(())
}

#[test]
fn revised_policy_cannot_drop_previous_negative_evidence() -> Result<(), Error> {
    let mut c = authorized()?;
    let mut r = receipt(1, Stage::Shadow);
    r.counts.missed_violations = 1;
    c.record(r)?;
    assert_eq!(
        c.prepare(candidate(), Stage::Shadow, 10)?.decision().reason,
        Reason::Negative
    );
    let mut p = policy();
    p.revision = 2;
    c.configure(Some(p), Controls::default())?;
    let mut r = receipt(2, Stage::Shadow);
    r.policy = 2;
    c.record(r)?;
    assert_eq!(
        c.prepare(candidate(), Stage::Shadow, 10)?.decision().reason,
        Reason::Negative
    );
    Ok(())
}
