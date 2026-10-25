#[allow(dead_code)]
#[path = "../examples/drift_eval/mod.rs"]
mod eval;
type Error = Box<dyn std::error::Error>;

#[test]
fn guarded_profile_preserves_seam_and_recovers_eleven_without_unsafe_dispatch() -> Result<(), Error>
{
    let report = eval::guarded::run()?;
    eval::compare(&report.seam, &eval::run()?)?;
    assert_eq!(report.healthy.len(), 42);
    let total = report
        .metrics
        .iter()
        .find(|m| m.class == "all" && m.label == "all")
        .ok_or("missing totals")?;
    assert_eq!(total.repairable_total, 14);
    assert_eq!(total.repairable_recovered, 11);
    assert_eq!(total.unsafe_writes, 0);
    assert_eq!(total.blocked, 25);
    assert_eq!(total.rejected, 6);
    for record in &report.healthy {
        assert_eq!(record.bypasses, 0);
        assert_eq!(record.dispatch_count, usize::from(record.bytes.is_some()));
        if record.case.recovered {
            assert_eq!(record.verification.as_slice(), ["passed"]);
            assert_eq!(record.decision, "Repaired");
        }
    }
    Ok(())
}

#[test]
fn overconfident_semantic_proposals_refuse_without_authority() -> Result<(), Error> {
    let report = eval::guarded::run()?;
    for class in [
        "unit_flip",
        "timestamp_epoch",
        "enum_expansion",
        "date_format",
    ] {
        let record = report
            .healthy
            .iter()
            .find(|r| r.case.id == format!("{class}-4"))
            .ok_or("missing wrong proposal")?;
        assert_eq!(record.verification, ["refused"]);
        assert!(record.facts.authority.is_none());
        assert_eq!(record.dispatch_count, 0);
        assert_eq!(record.decision, "Halt");
    }
    let misses: Vec<_> = report
        .healthy
        .iter()
        .filter(|r| r.case.label == eval::Label::Repairable && !r.case.recovered)
        .collect();
    assert_eq!(misses.len(), 3);
    assert!(
        misses
            .iter()
            .all(|r| ["null_semantics", "type_coercion"].contains(&r.case.class.as_str()))
    );
    Ok(())
}

#[test]
fn same_payload_and_confidence_require_independent_authority() -> Result<(), Error> {
    use eval::guarded::{
        Fault, execute,
        facts::{Authority, Facts},
    };
    let mut fixtures = eval::fixtures()?;
    let fixture = fixtures.first_mut().ok_or("missing fixture")?;
    let facts = Facts {
        authority: Some(Authority::Scale { factor: 0.01 }),
        preserve_other_fields: true,
    };
    let good = execute(&fixture.class, &fixture.task, &facts, Fault::Healthy)?;
    assert_eq!(good.verification, ["passed"]);
    assert_eq!(good.dispatch_count, 1);
    for authority in [None, Some(Authority::Scale { factor: 0.1 })] {
        let other = Facts {
            authority,
            ..facts.clone()
        };
        let refused = execute(&fixture.class, &fixture.task, &other, Fault::Healthy)?;
        assert_eq!(refused.verification, ["refused"]);
        assert_eq!(refused.dispatch_count, 0);
    }
    // Same authority, same JSON type and confidence, wrong proposed value.
    fixture.task.proposal.as_mut().ok_or("proposal")?.value = serde_json::json!(120);
    let wrong = execute(&fixture.class, &fixture.task, &facts, Fault::Healthy)?;
    assert_eq!(wrong.verification, ["refused"]);
    assert_eq!(wrong.dispatch_count, 0);
    Ok(())
}

#[test]
fn correct_target_with_lost_or_added_context_fails_verification_and_scoring() -> Result<(), Error> {
    use eval::guarded::facts::{Authority, Facts, verify};
    let fixture = eval::fixtures()?.remove(0);
    let facts = Facts {
        authority: Some(Authority::Scale { factor: 0.01 }),
        preserve_other_fields: true,
    };
    let mut patched = fixture.task.input.clone();
    patched["value"] = serde_json::json!(12);
    assert!(verify(
        &fixture.task.input,
        &facts,
        &serde_json::to_vec(&patched)?,
        &[0]
    ));
    for key in ["account", "memo", "id"] {
        let mut corrupt = patched.clone();
        corrupt.as_object_mut().ok_or("object")?.remove(key);
        assert!(!verify(
            &fixture.task.input,
            &facts,
            &serde_json::to_vec(&corrupt)?,
            &[0]
        ));
        assert!(!eval::score(&fixture, "guarded", Some(corrupt), vec![]).recovered);
    }
    patched["extra"] = serde_json::json!(true);
    assert!(!verify(
        &fixture.task.input,
        &facts,
        &serde_json::to_vec(&patched)?,
        &[0]
    ));
    assert!(!verify(&fixture.task.input, &facts, b"{}", &[]));
    Ok(())
}

#[test]
fn faults_preserve_original_bytes_outcomes_and_one_bypass_separately() -> Result<(), Error> {
    use eval::guarded::Fault;
    let report = eval::guarded::run()?;
    assert_eq!(report.faults.len(), 210);
    for r in &report.faults {
        if r.bypasses > 0 {
            assert_eq!(r.bypasses, 1);
            assert_eq!(r.dispatch_count, 1);
            assert!(r.original_bytes && r.original_outcome);
        }
    }
    for fault in [Fault::Error, Fault::Panic, Fault::NotReady, Fault::Budget] {
        let r = report
            .faults
            .iter()
            .find(|r| r.case.id == "unit_flip-1" && r.fault == fault)
            .ok_or("missing fault")?;
        assert_eq!(r.bypasses, 1);
        assert_eq!(r.case.outcome, eval::Outcome::UnsafeWrite);
        assert_eq!(r.verification.is_empty(), fault == Fault::NotReady);
    }
    let refusal = report
        .faults
        .iter()
        .find(|r| r.case.id == "unit_flip-1" && r.fault == Fault::Refuse)
        .ok_or("missing refusal")?;
    assert_eq!(refusal.bypasses, 0);
    assert_eq!(refusal.dispatch_count, 0);
    assert_eq!(refusal.verification, ["refused"]);
    assert!(report.fault_metrics.iter().any(|m| m.unsafe_writes > 0));
    Ok(())
}

#[test]
fn gate_catches_missing_evidence_relabeling_and_always_blocking() -> Result<(), Error> {
    let previous = eval::guarded::run()?;
    eval::guarded::compare(&previous, &previous)?;
    for mutation in 0..7 {
        let mut current = previous.clone();
        let record = current
            .healthy
            .iter_mut()
            .find(|r| r.case.recovered)
            .ok_or("recovery")?;
        match mutation {
            0 => record.verification.clear(),
            1 => record.facts.authority = None,
            2 => {
                record.bytes = Some(b"{\"value\":999}".to_vec());
            }
            3 => {
                current.metrics.clear();
            }
            4 => {
                current.version = 2;
            }
            5 => {
                current.profile = "conditional-v2".into();
            }
            _ => {
                current.healthy.clear();
            }
        }
        assert!(
            eval::guarded::compare(&current, &previous).is_err(),
            "mutation {mutation}"
        );
    }
    let mut current = previous.clone();
    // Consistent always-block report still loses recovery, even after rescoring.
    for (r, f) in current.healthy.iter_mut().zip(eval::fixtures()?) {
        r.bytes = None;
        r.dispatch_count = 0;
        r.original_outcome = false;
        r.verification.clear();
        r.verification_input = None;
        r.decision = "Halt".into();
        r.case = eval::score(&f, "guarded", None, vec![]);
    }
    current.metrics = eval::guarded::outcome_metrics(&current.healthy);
    current.verification_metrics = eval::guarded::verification_metrics(&current.healthy);
    eval::guarded::compare(&current, &current)?;
    assert!(eval::guarded::compare(&current, &previous).is_err());
    Ok(())
}

#[test]
fn guarded_report_roundtrips_and_keeps_a_distinct_qualified_baseline() -> Result<(), Error> {
    let current = eval::guarded::run()?;
    let qualified = serde_json::from_str(include_str!(
        "../examples/drift_eval/guarded/baseline.v3.json"
    ))?;
    eval::guarded::compare(&current, &qualified)?;
    let mut missing = serde_json::to_value(&current)?;
    missing
        .as_object_mut()
        .ok_or("report object")?
        .remove("verification_metrics");
    assert!(serde_json::from_value::<eval::guarded::Report>(missing).is_err());
    let saved = serde_json::from_slice(&serde_json::to_vec(&current)?)?;
    eval::guarded::compare(&current, &saved)?;
    assert!(
        serde_json::from_str::<eval::guarded::Report>(include_str!(
            "../examples/drift_eval/baseline.v2.json"
        ))
        .is_err()
    );
    Ok(())
}
