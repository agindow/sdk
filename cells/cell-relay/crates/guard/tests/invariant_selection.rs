#[path = "support/invariants.rs"]
mod fixture;
#[path = "support/statistical.rs"]
mod stats;
use agw_diff::{Change, DriftClass, Kind, Reason, Report, Side};
use agw_guard::statistical::{Decision, Engine, Operation, Plan};
use agw_invariants::{Form, Rule, Verdict, store::Controls};
use fixture::{BAD, CLEAN, Error};
fn range(field: &str) -> Rule {
    Rule {
        id: "gate".into(),
        form: Form::Range {
            field: field.into(),
            min: 0,
            max: 1,
        },
    }
}

#[test]
fn invalid_paths_and_scope_mismatch_are_rejected_administratively() -> Result<(), Error> {
    for path in ["amount", "/bad~2escape"] {
        assert!(fixture::select(&[range(path)], &[path], Controls::default()).is_err());
    }
    let mut identity = fixture::identity();
    identity.vendor = "other".into();
    assert!(
        Plan::new(identity, vec![])?
            .with_invariants(Side::Request, fixture::selection()?)
            .is_err()
    );
    assert!(
        Plan::new(fixture::identity(), vec![])?
            .with_invariants(Side::Request, fixture::selection()?)?
            .with_invariants(Side::Request, fixture::selection()?)
            .is_err()
    );
    Ok(())
}
#[test]
fn escaped_pointers_and_array_refusal_preserve_exact_projection() -> Result<(), Error> {
    let rules = [
        range("/a~1b/~0"),
        Rule {
            id: "array".into(),
            form: Form::Range {
                field: "/items/0".into(),
                min: 0,
                max: 1,
            },
        },
    ];
    let selection = fixture::select(&rules, &["/a~1b/~0", "/items/0"], Controls::default())?;
    let engine = Engine::statistical(
        Plan::new(fixture::identity(), vec![])?.with_invariants(Side::Request, selection)?,
    );
    let bytes: &[u8] = br#"{"a/b":{"~":2},"items":[2]}"#;
    let output = engine
        .begin_call(
            stats::context("observe")?,
            Operation::Write,
            "audit",
            100_000_000,
            0,
        )
        .dispatch(stats::body(bytes), None, || bytes, |b| stats::body(b));
    assert_eq!(output.request.invariants[0].verdict, Verdict::Violation);
    assert_eq!(output.request.invariants[1].verdict, Verdict::Unavailable);
    assert!(std::ptr::eq(output.outcome.ok_or("outcome")?, bytes));
    Ok(())
}
#[test]
fn customer_controls_and_frozen_call_survive_engine_replacement() -> Result<(), Error> {
    let rules = [range("/total")];
    let controls = Controls {
        pin: Some(2),
        ..Controls::default()
    };
    let selection = fixture::select(&rules, &["/total"], controls)?;
    let mut engine = Engine::statistical(
        Plan::new(fixture::identity(), vec![])?.with_invariants(Side::Request, selection)?,
    );
    let call = engine.begin_call(
        stats::context("repair")?,
        Operation::Write,
        "audit",
        100_000_000,
        0,
    );
    let disabled = fixture::select(
        &rules,
        &["/total"],
        Controls {
            disabled: vec!["gate".into()],
            ..Controls::default()
        },
    )?;
    engine
        .replace(Plan::new(fixture::identity(), vec![])?.with_invariants(Side::Request, disabled)?);
    let output = call.dispatch(
        stats::body(CLEAN),
        None,
        || panic!("old frozen rule must halt"),
        |_| stats::body(CLEAN),
    );
    assert_eq!(output.request.decision, Decision::Halt);
    let audit = serde_json::to_value(output.request.invariant_audit.ok_or("audit")?)?;
    assert_eq!(audit["proof"]["verified_against"], "pin");
    assert_eq!(audit["metadata"]["agindow_invariants"]["version"], 2);
    let output = engine
        .begin_call(
            stats::context("repair")?,
            Operation::Write,
            "audit",
            100_000_000,
            0,
        )
        .dispatch(stats::body(CLEAN), None, || CLEAN, |b| stats::body(b));
    assert_eq!(output.request.decision, Decision::Pass);
    assert!(output.request.invariants.is_empty());
    Ok(())
}
#[test]
fn mixed_shape_statistics_and_invariants_retain_semantic_halt() -> Result<(), Error> {
    let engine = Engine::statistical(
        Plan::new(
            fixture::identity(),
            vec![stats::field(Side::Request, "1", "100")?],
        )?
        .with_invariants(Side::Request, fixture::selection()?)?,
    );
    let report = Report {
        changes: vec![Change {
            side: Side::Request,
            path: "/old".into(),
            to_path: None,
            kind: Kind::Removed,
            class: DriftClass::Shape,
            reason: Reason::DeclaredStructure,
        }],
        skipped: None,
    };
    let mut body = stats::body(BAD);
    body.findings = Some(&report);
    let output = engine
        .begin_call(
            stats::context("repair")?,
            Operation::Write,
            "audit",
            100_000_000,
            0,
        )
        .dispatch(
            body,
            None,
            || panic!("semantic halt must dominate shape"),
            |_| stats::body(CLEAN),
        );
    assert_eq!(output.request.records.len(), 5);
    assert!(
        output
            .request
            .report
            .changes
            .iter()
            .any(|c| c.class == DriftClass::Shape)
    );
    assert_eq!(output.request.invariants[0].verdict, Verdict::Violation);
    assert_eq!(output.request.decision, Decision::Halt);
    assert_eq!(output.bypasses, 0);
    Ok(())
}
