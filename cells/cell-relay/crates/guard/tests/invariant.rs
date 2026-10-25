#[path = "support/invariants.rs"]
mod fixture;
#[path = "support/statistical.rs"]
mod stats;
use agw_guard::statistical::{Decision, Operation};
use agw_invariants::Verdict;
use fixture::{BAD, CLEAN, Error, NOVEL};

#[test]
fn matrix_evaluates_both_phases_and_halts_before_dispatch() -> Result<(), Error> {
    for mode in ["observe", "repair", "strict"] {
        for operation in [Operation::Read, Operation::Write, Operation::Unknown] {
            let expected = if mode == "observe" {
                Decision::Pass
            } else if mode == "repair" && operation == Operation::Read {
                Decision::Degraded
            } else {
                Decision::Halt
            };
            let engine = fixture::engine()?;
            let original = CLEAN;
            let calls = std::cell::Cell::new(0);
            let output = engine
                .begin_call(stats::context(mode)?, operation, "audit", 100_000_000, 0)
                .dispatch(
                    stats::body(BAD),
                    None,
                    || {
                        calls.set(calls.get() + 1);
                        original
                    },
                    |b| stats::body(b),
                );
            assert_eq!(output.request.decision, expected);
            assert_eq!(calls.get(), usize::from(expected != Decision::Halt));
            assert_eq!(output.request.invariants.len(), 6);
            assert_eq!(output.request.invariants[0].verdict, Verdict::Violation);
            assert_eq!(output.bypasses, 0);
            if expected != Decision::Halt {
                assert!(std::ptr::eq(output.outcome.ok_or("outcome")?, original));
                assert_eq!(output.response.ok_or("response")?.invariants.len(), 6);
            }
        }
    }
    Ok(())
}

#[test]
fn response_halt_preserves_completed_outcome_and_actual_expected_audit() -> Result<(), Error> {
    let output = fixture::engine()?
        .begin_call(
            stats::context("repair")?,
            Operation::Write,
            "audit",
            100_000_000,
            0,
        )
        .dispatch(
            stats::body(CLEAN),
            None,
            || BAD.to_vec(),
            |b| stats::body(b),
        );
    assert!(output.dispatched);
    assert_eq!(output.outcome.as_deref(), Some(BAD));
    let phase = output.response.ok_or("response")?;
    assert_eq!(phase.decision, Decision::Halt);
    assert_eq!(phase.headers.get("X-Agindow-Drift-Class"), Some("semantic"));
    assert!(phase.warning.is_some());
    let audit = phase.invariant_audit.ok_or("audit")?;
    let serialized = serde_json::to_value(&audit)?;
    assert_eq!(serialized["drift"]["class"], "semantic");
    assert_eq!(serialized["drift"]["side"], "response");
    assert_eq!(serialized["drift"]["changes"][0]["path"], "rule-0");
    assert_eq!(
        serialized["drift"]["changes"][0]["kind"],
        "invariant_violated"
    );
    assert_eq!(serialized["drift"]["changes"][0]["from"], "118");
    assert!(
        serialized["drift"]["changes"][0]["to"]
            .as_str()
            .is_some_and(|s| s.contains("120"))
    );
    assert_eq!(serialized["proof"]["contract_version"], "7");
    assert_eq!(serialized["proof"]["write_integrity"], false);
    assert_eq!(serialized["patch"]["byte_identical"], true);
    assert_eq!(serialized["decision"], "blocked");
    assert!(
        serde_json::from_value::<agw_audit::contracts::audit_artifact::AuditArtifact>(serialized)
            .is_ok()
    );
    Ok(())
}

#[test]
fn enum_addition_is_semantic_info_and_never_an_invariant_violation() -> Result<(), Error> {
    let output = fixture::engine()?
        .begin_call(
            stats::context("repair")?,
            Operation::Write,
            "audit",
            100_000_000,
            0,
        )
        .dispatch(
            stats::body(NOVEL),
            None,
            || panic!("must halt novel enum write"),
            |_| stats::body(CLEAN),
        );
    assert_eq!(output.request.decision, Decision::Halt);
    assert_eq!(output.request.invariants[4].verdict, Verdict::EnumAddition);
    assert!(
        output
            .request
            .invariants
            .iter()
            .all(|r| r.verdict != Verdict::Violation)
    );
    let audit = serde_json::to_value(output.request.invariant_audit.ok_or("audit")?)?;
    assert_eq!(audit["drift"]["changes"][0]["kind"], "enum_expanded");
    assert_eq!(
        audit["metadata"]["agindow_invariants"]["records"][4]["severity"],
        "info"
    );
    Ok(())
}

#[test]
fn unavailable_rules_and_tolerance_do_not_invent_violations() -> Result<(), Error> {
    let bytes: &[u8] = br#"{"total":119,"amount":100,"tax":20}"#;
    let output = fixture::engine()?
        .begin_call(
            stats::context("repair")?,
            Operation::Read,
            "audit",
            100_000_000,
            0,
        )
        .dispatch(stats::body(bytes), None, || bytes, |b| stats::body(b));
    assert_eq!(output.request.invariants[0].verdict, Verdict::Pass);
    assert_eq!(output.request.invariants[3].verdict, Verdict::Unavailable);
    assert_eq!(output.request.unavailable_invariants, 3);
    assert_eq!(output.request.decision, Decision::Pass);
    let audit = output.request.invariant_audit.ok_or("audit")?;
    assert!(audit.drift.is_none());
    assert!(!audit.proof.write_integrity);
    let decimal: &[u8] = br#"{"total":1e2,"amount":80,"tax":20}"#;
    let output = fixture::engine()?
        .begin_call(
            stats::context("repair")?,
            Operation::Write,
            "audit",
            100_000_000,
            0,
        )
        .dispatch(stats::body(decimal), None, || decimal, |b| stats::body(b));
    assert_eq!(output.request.invariants[0].verdict, Verdict::Unavailable);
    Ok(())
}

#[test]
fn unchanged_enum_meaning_constraints_and_new_values_keep_separate_evidence() -> Result<(), Error> {
    let bytes: &[u8] = br#"{"total":118,"amount":100,"tax":20,"created":1,"updated":2,"status":"refunded","correlated_total":118}"#;
    let output = fixture::engine()?
        .begin_call(
            stats::context("repair")?,
            Operation::Write,
            "audit",
            100_000_000,
            0,
        )
        .dispatch(
            stats::body(bytes),
            None,
            || panic!("semantic write must halt"),
            |_| stats::body(CLEAN),
        );
    assert_eq!(output.request.invariants[0].verdict, Verdict::Violation);
    assert_eq!(output.request.invariants[4].verdict, Verdict::EnumAddition);
    let audit = serde_json::to_value(output.request.invariant_audit.ok_or("audit")?)?;
    let changes = audit["drift"]["changes"].as_array().ok_or("changes")?;
    assert_eq!(changes.len(), 2);
    assert_eq!(changes[0]["kind"], "invariant_violated");
    assert_eq!(changes[1]["kind"], "enum_expanded");
    Ok(())
}
