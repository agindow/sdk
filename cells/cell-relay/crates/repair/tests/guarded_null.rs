#[allow(dead_code)]
#[path = "../examples/drift_eval/mod.rs"]
mod eval;
use eval::guarded::{
    Fault, execute,
    facts::{Authority, Facts},
};
type Error = Box<dyn std::error::Error>;

#[test]
fn explicit_empty_null_mapping_reaches_independent_verification_before_dispatch()
-> Result<(), Error> {
    let fixture = eval::fixtures()?
        .into_iter()
        .find(|f| f.id == "null_semantics-2")
        .ok_or("case")?;
    let facts = Facts {
        authority: Some(Authority::Mapping {
            source: serde_json::json!(""),
            destination: serde_json::Value::Null,
        }),
        preserve_other_fields: true,
    };
    let healthy = execute(&fixture.class, &fixture.task, &facts, Fault::Healthy)?;
    assert_eq!(healthy.verification, ["passed"]);
    assert_eq!(healthy.dispatch_count, 1);
    assert!(healthy.case.recovered && !healthy.case.violation);
    for authority in [
        None,
        Some(Authority::Mapping {
            source: serde_json::json!("other"),
            destination: serde_json::Value::Null,
        }),
    ] {
        let refused = execute(
            &fixture.class,
            &fixture.task,
            &Facts {
                authority,
                ..facts.clone()
            },
            Fault::Healthy,
        )?;
        assert_eq!(refused.verification, ["refused"]);
        assert_eq!(refused.dispatch_count, 0);
        assert_eq!(refused.decision, "Halt");
    }
    for fault in [Fault::Error, Fault::Panic, Fault::Budget, Fault::NotReady] {
        let bypass = execute(&fixture.class, &fixture.task, &facts, fault)?;
        assert_eq!(bypass.bypasses, 1);
        assert!(bypass.original_bytes && bypass.original_outcome);
        assert_eq!(bypass.case.outcome, eval::Outcome::UnsafeWrite);
    }
    Ok(())
}
#[test]
fn diagnosis_reconciles_all_cases_and_preserves_the_numeric_fence() -> Result<(), Error> {
    let report = eval::guarded::run()?;
    let diagnosis = eval::guarded::diagnostics::diagnose(&report)?;
    assert_eq!(diagnosis.total, 42);
    assert_eq!(diagnosis.repairable_total, 14);
    assert_eq!(diagnosis.without_unique_evidence, 28);
    assert_eq!(diagnosis.repairable_numeric_refusals, 3);
    assert_eq!(diagnosis.counts.values().sum::<usize>(), 42);
    assert_eq!(diagnosis.counts.get("recovered"), Some(&11));
    assert_eq!(diagnosis.counts.get("missing_proposal"), Some(&14));
    let misses: Vec<_> = diagnosis
        .cases
        .iter()
        .filter(|c| c.label == eval::Label::Repairable && c.cause != "recovered")
        .collect();
    assert_eq!(misses.len(), 3);
    assert!(misses.iter().all(|c| c.cause == "numeric_coercion_fence"));
    Ok(())
}
