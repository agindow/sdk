#[path = "../examples/drift_eval/mod.rs"]
mod eval;
use eval::{Label, Outcome};
type Error = Box<dyn std::error::Error>;

#[test]
fn independently_labeled_corpus_covers_every_class() -> Result<(), Error> {
    let fixtures = eval::fixtures()?;
    assert_eq!(fixtures.len(), 42);
    for class in eval::CLASSES {
        for label in [Label::Repairable, Label::Ambiguous, Label::Unsupported] {
            assert_eq!(
                fixtures
                    .iter()
                    .filter(|f| f.class == class && f.oracle.label == label)
                    .count(),
                2
            );
        }
    }
    assert!(fixtures.iter().all(|f| !f.oracle.rationale.is_empty()));
    Ok(())
}

#[test]
fn full_payload_baseline_preserves_context_and_recovers_repairable_cases() -> Result<(), Error> {
    let report = eval::run()?;
    let cases: Vec<_> = report
        .cases
        .iter()
        .filter(|c| c.arm == "naive" && c.label == Label::Repairable)
        .collect();
    assert_eq!(cases.len(), 14);
    assert!(
        cases
            .iter()
            .all(|c| c.recovered && c.successful_repair && !c.violation)
    );
    assert!(report.cases.iter().any(|c| c.arm == "naive" && c.violation));
    Ok(())
}

#[test]
fn unsafe_whole_write_cannot_earn_recovery_for_a_correct_target() -> Result<(), Error> {
    let f = eval::fixtures()?.remove(0);
    let mut body = f.oracle.expected.clone();
    body.as_object_mut()
        .ok_or("expected object")?
        .remove("memo");
    let scored = eval::score(&f, "naive", Some(body), vec![]);
    assert!(scored.target_recovered);
    assert!(!scored.recovered);
    assert_eq!(scored.outcome, Outcome::UnsafeWrite);
    let scored = eval::score(&f, "selective", None, vec![]);
    assert_eq!(scored.outcome, Outcome::Blocked);
    assert!(!scored.recovered && !scored.violation);
    Ok(())
}

#[test]
fn vendor_acceptance_is_separate_from_intent() -> Result<(), Error> {
    let f = eval::fixtures()?.remove(0);
    let mut body = f.oracle.expected.clone();
    body["value"] = serde_json::json!(999);
    assert_eq!(
        eval::score(&f, "naive", Some(body.clone()), vec![]).outcome,
        Outcome::UnsafeWrite
    );
    body["value"] = serde_json::json!("wrong JSON type");
    assert_eq!(
        eval::score(&f, "naive", Some(body), vec![]).outcome,
        Outcome::Rejected
    );
    Ok(())
}

#[test]
fn wrong_overconfident_proposals_are_scored_without_requiring_native_failures() -> Result<(), Error>
{
    let fixtures = eval::fixtures()?;
    let faults: Vec<_> = fixtures
        .iter()
        .filter(|f| {
            f.oracle.label == Label::Ambiguous
                && f.task
                    .proposal
                    .as_ref()
                    .is_some_and(|p| p.confidence > 0.99)
        })
        .collect();
    assert_eq!(faults.len(), 7);
    for f in faults {
        let proposal = f.task.proposal.as_ref().ok_or("missing proposal")?;
        let mut body = f.task.input.clone();
        let object = body.as_object_mut().ok_or("object input")?;
        if f.task.rename {
            let source = object.remove("value").ok_or("missing source")?;
            object.insert(
                proposal.rename_to.clone().ok_or("missing destination")?,
                source,
            );
        } else {
            object.insert(f.task.target.clone(), proposal.value.clone());
        }
        let scored = eval::score(f, "naive", Some(body), vec![]);
        assert!(!scored.recovered);
        assert!(scored.violation || scored.outcome == Outcome::Rejected);
    }
    let report = eval::run()?;
    assert!(
        report
            .cases
            .iter()
            .filter(|c| c.arm == "selective")
            .all(|c| !c.trace.is_empty())
    );
    assert!(eval::summary(&report).contains("repairable recovery"));
    Ok(())
}

#[test]
fn gate_rejects_new_unsafe_writes_lost_recovery_and_always_blocking() -> Result<(), Error> {
    let previous = eval::run()?;
    eval::compare(&previous, &previous)?;
    let index = previous
        .cases
        .iter()
        .position(|c| c.recovered)
        .ok_or("no recovered case")?;
    let mut current = previous.clone();
    let case = current.cases.get_mut(index).ok_or("missing case")?;
    case.outcome = Outcome::Blocked;
    case.recovered = false;
    case.target_recovered = false;
    case.successful_repair = false;
    case.sent = None;
    current.refresh_metrics();
    assert!(eval::compare(&current, &previous).is_err());
    let mut current = previous.clone();
    let case = current.cases.get_mut(index).ok_or("missing case")?;
    case.outcome = Outcome::UnsafeWrite;
    case.recovered = false;
    case.violation = true;
    case.successful_repair = false;
    case.target_recovered = false;
    case.sent.as_mut().ok_or("missing payload")?["value"] = serde_json::json!(999);
    current.refresh_metrics();
    assert!(eval::compare(&current, &previous).is_err());
    let mut current = previous.clone();
    for case in current.cases.iter_mut().filter(|c| c.arm == "selective") {
        case.outcome = Outcome::Blocked;
        case.recovered = false;
        case.target_recovered = false;
        case.violation = false;
        case.successful_repair = false;
        case.sent = None;
    }
    current.refresh_metrics();
    let m = current
        .metrics
        .iter()
        .find(|m| m.arm == "selective" && m.class == "all" && m.label == "all")
        .ok_or("missing totals")?;
    assert!(m.violations_per_dispatch.is_none());
    assert_eq!(m.repairable_total, 14);
    assert_eq!(m.repairable_recovery_rate, Some(0.0));
    assert!(eval::compare(&current, &previous).is_err());
    Ok(())
}

#[test]
fn metrics_reconcile_outcomes_and_keep_repairable_denominators() -> Result<(), Error> {
    let report = eval::run()?;
    for arm in eval::ARMS {
        let m = report
            .metrics
            .iter()
            .find(|m| m.arm == arm && m.class == "all" && m.label == "all")
            .ok_or("missing totals")?;
        assert_eq!(m.total, 42);
        assert_eq!(m.repairable_total, 14);
        assert_eq!(
            m.safe_writes + m.unsafe_writes + m.blocked + m.rejected,
            m.total
        );
        assert_eq!(
            m.repairable_recovery_rate,
            Some(m.repairable_recovered as f64 / 14.0)
        );
        assert_eq!(m.violation_rate, m.unsafe_writes as f64 / 42.0);
    }
    let mut invalid = report.clone();
    invalid.metrics.clear();
    assert!(eval::compare(&invalid, &report).is_err());
    let mut json = serde_json::to_value(&report)?;
    json["metrics"][0]
        .as_object_mut()
        .ok_or("metric object")?
        .remove("violation_rate");
    assert!(serde_json::from_value::<eval::Report>(json).is_err());
    Ok(())
}

#[test]
fn qualified_v2_baseline_is_reproducible_and_v1_is_incompatible() -> Result<(), Error> {
    let current = eval::run()?;
    let previous = serde_json::from_str(include_str!("../examples/drift_eval/baseline.v2.json"))?;
    eval::compare(&current, &previous)?;
    let mut changed = current.clone();
    changed.version = 1;
    assert!(eval::compare(&current, &changed).is_err());
    changed = current.clone();
    changed.corpus.push(' ');
    assert!(eval::compare(&current, &changed).is_err());
    Ok(())
}

#[test]
fn report_metrics_survive_json_roundtrip() -> Result<(), Error> {
    let current = eval::run()?;
    let saved = serde_json::from_slice(&serde_json::to_vec(&current)?)?;
    eval::compare(&current, &saved)?;
    Ok(())
}

#[test]
fn report_cannot_relabel_an_unsafe_sent_payload_as_recovered() -> Result<(), Error> {
    let previous = eval::run()?;
    let mut current = previous.clone();
    let case = current
        .cases
        .iter_mut()
        .find(|c| c.violation)
        .ok_or("no unsafe case")?;
    case.outcome = Outcome::SafeWrite;
    case.violation = false;
    case.recovered = true;
    case.target_recovered = true;
    current.refresh_metrics();
    assert!(eval::compare(&current, &previous).is_err());
    Ok(())
}
