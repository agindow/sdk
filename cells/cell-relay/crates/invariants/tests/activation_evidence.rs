#[path = "support/activation.rs"]
mod fixture;
use agw_invariants::{Form, Scalar, Verdict, activation::*, store::Controls};
use fixture::*;

#[test]
fn calibration_is_candidate_specific_and_synthetic_evidence_cannot_grant_blocking()
-> Result<(), Error> {
    for production in [false, true] {
        let mut c = authorized()?;
        let mut p = policy();
        p.calibration = Some(Calibration {
            dataset: "modeled-host-attestation".into(),
            candidate: 1,
            sampling: "independent calls; complete comparable populations".into(),
            independent_production: production,
            counts: receipt(1, Stage::Shadow).counts,
        });
        // This tests the trusted host attestation gate, not production calibration.
        p.revision = 2;
        c.configure(Some(p), Controls::default())?;
        for (id, stage, at) in [
            (1, Stage::Shadow, 10),
            (2, Stage::Reads, 20),
            (3, Stage::Blocking, 30),
        ] {
            let mut r = receipt(id, stage);
            r.policy = 2;
            if stage == Stage::Blocking {
                r.started = 21;
                r.finished = 25;
            }
            c.record(r)?;
            let pending = c.prepare(candidate(), stage, at)?;
            if stage == Stage::Blocking && !production {
                assert_eq!(pending.decision().reason, Reason::Calibration);
            } else {
                assert_eq!(pending.decision().reason, Reason::Qualified);
                c.publish(pending, at)?;
            }
        }
        assert_eq!(
            c.active().ok_or(Error::Invalid)?.stage(),
            if production {
                Stage::Blocking
            } else {
                Stage::Reads
            }
        );
    }
    Ok(())
}

#[test]
fn boundary_rates_are_inclusive_and_unsafe_windows_cannot_be_diluted() -> Result<(), Error> {
    for (false_positives, expected) in [(1, Reason::Qualified), (2, Reason::Negative)] {
        let mut p = policy();
        p.thresholds.false_positive_ppm = 100_000;
        let mut c = Controller::new(scope(), 32)?;
        c.configure(Some(p), Controls::default())?;
        let mut r = receipt(1, Stage::Shadow);
        r.counts.false_positives = false_positives;
        c.record(r)?;
        assert_eq!(
            c.prepare(candidate(), Stage::Shadow, 10)?.decision().reason,
            expected
        );
    }
    let mut c = authorized()?;
    let mut bad = receipt(1, Stage::Shadow);
    bad.counts.false_positives = 1;
    c.record(bad)?;
    let mut good = receipt(2, Stage::Shadow);
    good.counts.valid = 1_000_000;
    good.counts.evaluated = 1_000_010;
    good.counts.eligible = 1_000_010;
    c.record(good)?;
    assert_eq!(
        c.prepare(candidate(), Stage::Shadow, 10)?.decision().reason,
        Reason::Negative
    );
    Ok(())
}

#[test]
fn explicit_override_reconciliation_survives_revocation_and_cannot_auto_promote()
-> Result<(), Error> {
    let mut c = authorized()?;
    c.record(receipt(1, Stage::Shadow))?;
    c.publish_prepared(candidate(), Stage::Shadow, 10)?;
    let mut replacement = candidate().rules.remove(0);
    replacement.form = Form::Range {
        field: "/n".into(),
        min: 0,
        max: 20,
    };
    c.configure(
        None,
        Controls {
            overrides: vec![replacement.clone()],
            ..Default::default()
        },
    )?;
    c.reconcile()?;
    assert_eq!(
        c.active().ok_or(Error::Invalid)?.snapshot().program.rules()[0]
            .evaluate(&[Scalar::Integer(11)]),
        Verdict::Pass
    );
    let mut p = policy();
    p.revision = 2;
    c.configure(
        Some(p),
        Controls {
            overrides: vec![replacement],
            ..Default::default()
        },
    )?;
    assert_eq!(
        c.prepare(candidate(), Stage::Reads, 20)?.decision().reason,
        Reason::CustomerControl
    );
    Ok(())
}

#[test]
fn revoked_revision_and_reused_holdout_cannot_authorize_a_new_stage() -> Result<(), Error> {
    let mut c = authorized()?;
    c.configure(None, Controls::default())?;
    assert_eq!(
        c.configure(Some(policy()), Controls::default()),
        Err(Error::Invalid)
    );
    let mut c = authorized()?;
    c.record(receipt(1, Stage::Shadow))?;
    c.publish_prepared(candidate(), Stage::Shadow, 10)?;
    let mut reused = receipt(2, Stage::Reads);
    reused.holdout = receipt(1, Stage::Shadow).holdout;
    c.record(reused)?;
    assert_eq!(
        c.prepare(candidate(), Stage::Reads, 20)?.decision().reason,
        Reason::Contradictory
    );
    Ok(())
}

#[test]
fn relabeling_an_unchanged_candidate_cannot_erase_negative_windows() -> Result<(), Error> {
    let mut c = authorized()?;
    let mut negative = receipt(1, Stage::Shadow);
    negative.counts.false_positives = 1;
    c.record(negative)?;
    assert_eq!(
        c.prepare(candidate(), Stage::Shadow, 10)?.decision().reason,
        Reason::Negative
    );
    let mut next = candidate();
    next.version = 2;
    let mut good = receipt(2, Stage::Shadow);
    good.candidate = 2;
    c.record(good)?;
    assert_eq!(
        c.prepare(next, Stage::Shadow, 10)?.decision().reason,
        Reason::Negative
    );
    Ok(())
}

#[test]
fn fresh_shadow_generation_cannot_silently_weaken_known_read_protection() -> Result<(), Error> {
    let mut c = authorized()?;
    c.record(receipt(1, Stage::Shadow))?;
    c.publish_prepared(candidate(), Stage::Shadow, 10)?;
    c.record(receipt(2, Stage::Reads))?;
    c.publish_prepared(candidate(), Stage::Reads, 20)?;
    let mut next = candidate();
    next.version = 2;
    assert_eq!(
        c.prepare(next, Stage::Shadow, 30)?.decision().reason,
        Reason::CustomerControl
    );
    let mut narrower = policy();
    narrower.revision = 2;
    narrower.modes = vec![RolloutMode::Observe];
    c.configure(Some(narrower), Controls::default())?;
    assert_eq!(
        c.prepare(candidate(), Stage::Blocking, 30)?
            .decision()
            .reason,
        Reason::CustomerControl
    );
    assert_eq!(c.active().ok_or(Error::Invalid)?.stage(), Stage::Reads);
    Ok(())
}

#[test]
fn a_stalled_prepared_publication_rechecks_expiry_at_commit() -> Result<(), Error> {
    let mut c = authorized()?;
    c.record(receipt(1, Stage::Shadow))?;
    let pending = c.prepare(candidate(), Stage::Shadow, 10)?;
    assert_eq!(c.publish(pending, 100), Err(Error::Expired));
    assert!(c.active().is_none());
    assert_eq!(
        c.decisions().last().ok_or(Error::Invalid)?.reason,
        Reason::Expired
    );
    Ok(())
}

#[test]
fn undersized_negative_windows_cannot_hide_below_sample_floors() -> Result<(), Error> {
    let mut p = policy();
    p.thresholds.false_positive_ppm = 100_000;
    let mut c = Controller::new(scope(), 32)?;
    c.configure(Some(p), Controls::default())?;
    let mut bad = receipt(1, Stage::Shadow);
    bad.counts = Counts {
        eligible: 2,
        evaluated: 2,
        valid: 1,
        invalid: 1,
        false_positives: 1,
        missed_violations: 0,
    };
    c.record(bad)?;
    c.record(receipt(2, Stage::Shadow))?;
    assert_eq!(
        c.prepare(candidate(), Stage::Shadow, 10)?.decision().reason,
        Reason::Negative
    );
    Ok(())
}

#[test]
fn reordering_rules_and_bindings_does_not_erase_adverse_evidence() -> Result<(), Error> {
    let mut first = candidate();
    let mut second_rule = first.rules[0].clone();
    second_rule.id = "second".into();
    first.rules.push(second_rule.clone());
    first.bindings.push("/unused".into());
    let mut p = policy();
    p.attestations.push(Attestation {
        rule: second_rule,
        semantics: "explicit customer bounds".into(),
    });
    let mut c = Controller::new(scope(), 32)?;
    c.configure(Some(p), Controls::default())?;
    let mut bad = receipt(1, Stage::Shadow);
    bad.counts.false_positives = 1;
    c.record(bad)?;
    assert_eq!(
        c.prepare(first.clone(), Stage::Shadow, 10)?
            .decision()
            .reason,
        Reason::Negative
    );
    first.version = 2;
    first.rules.reverse();
    first.bindings.reverse();
    let mut good = receipt(2, Stage::Shadow);
    good.candidate = 2;
    c.record(good)?;
    assert_eq!(
        c.prepare(first, Stage::Shadow, 10)?.decision().reason,
        Reason::Negative
    );
    Ok(())
}

#[test]
fn authorization_and_pending_controls_obey_the_dsl_resource_bounds() -> Result<(), Error> {
    let mut p = policy();
    p.attestations[0].rule.form = Form::Enum {
        field: "/n".into(),
        values: (0..129).map(|i| i.to_string()).collect(),
    };
    let mut c = Controller::new(scope(), 32)?;
    assert_eq!(
        c.configure(Some(p), Controls::default()),
        Err(Error::Invalid)
    );
    let mut excessive = candidate().rules.remove(0);
    excessive.form = Form::Sum {
        total: "/n".into(),
        terms: vec!["/n".into(); 33],
        tolerance: 0,
    };
    assert_eq!(
        c.configure(
            Some(policy()),
            Controls {
                overrides: vec![excessive],
                ..Default::default()
            }
        ),
        Err(Error::Invalid)
    );
    Ok(())
}

#[test]
fn policy_refresh_cannot_hide_historical_timestamp_contradictions() -> Result<(), Error> {
    let mut c = authorized()?;
    let mut bad = receipt(1, Stage::Shadow);
    bad.started = 6;
    bad.finished = 5;
    c.record(bad)?;
    assert_eq!(
        c.prepare(candidate(), Stage::Shadow, 10)?.decision().reason,
        Reason::Contradictory
    );
    let mut p = policy();
    p.revision = 2;
    c.configure(Some(p), Controls::default())?;
    let mut good = receipt(2, Stage::Shadow);
    good.policy = 2;
    c.record(good)?;
    assert_eq!(
        c.prepare(candidate(), Stage::Shadow, 10)?.decision().reason,
        Reason::Contradictory
    );
    Ok(())
}

#[test]
fn rejected_revocation_at_the_revision_limit_is_atomic() -> Result<(), Error> {
    let mut c = Controller::new(scope(), 32)?;
    let mut p = policy();
    p.revision = u64::MAX;
    c.configure(Some(p), Controls::default())?;
    let mut r = receipt(1, Stage::Shadow);
    r.policy = u64::MAX;
    c.record(r)?;
    let pending = c.prepare(candidate(), Stage::Shadow, 10)?;
    assert_eq!(c.configure(None, Controls::default()), Err(Error::Capacity));
    c.publish(pending, 10)?;
    Ok(())
}

#[test]
fn conflicting_duplicate_and_full_ledger_cannot_be_ignored_before_publication() -> Result<(), Error>
{
    for full in [false, true] {
        let mut c = Controller::new(scope(), if full { 2 } else { 32 })?;
        c.configure(Some(policy()), Controls::default())?;
        c.record(receipt(1, Stage::Shadow))?;
        if full {
            c.record(receipt(2, Stage::Shadow))?;
        }
        let pending = c.prepare(candidate(), Stage::Shadow, 10)?;
        let mut bad = receipt(if full { 3 } else { 1 }, Stage::Shadow);
        bad.counts.false_positives = 1;
        assert_eq!(
            c.record(bad.clone()),
            Err(if full {
                Error::Capacity
            } else {
                Error::Conflict
            })
        );
        assert_eq!(c.publish(pending, 10), Err(Error::Conflict));
        assert!(c.active().is_none());
        let rejected = c.prepare(candidate(), Stage::Shadow, 10)?;
        assert_eq!(
            rejected.decision().reason,
            if full {
                Reason::Incomplete
            } else {
                Reason::Contradictory
            }
        );
        assert!(rejected.decision().receipts.contains(&bad));
    }
    Ok(())
}
