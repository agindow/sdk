mod support;
use agw_diff::{DriftClass, Kind, Reason};
use agw_repair::{Mode, Reject, Status};
use support::{replacement, run, unchanged};

fn plan() -> (agw_diff::Report, agw_repair::Candidate) {
    let (mut report, candidate) = replacement("value", b"null", DriftClass::Semantic);
    if let Some(change) = report.changes.first_mut() {
        change.kind = Kind::NullMeaningChanged;
        change.reason = Reason::MeaningChanged;
    }
    (report, candidate)
}
#[test]
fn explicit_empty_string_null_mapping_preserves_every_other_byte() {
    let bytes = br#" { "value" : "", "id" : 9007199254740993, "raw":1e400 } "#;
    let (report, candidate) = plan();
    let result = run(bytes, &report, &[candidate], Mode::Repair);
    assert_eq!(
        result.body.as_ref(),
        br#" { "value" : null, "id" : 9007199254740993, "raw":1e400 } "#
    );
    assert_eq!(result.assessments[0].status, Status::Applied);
    assert!(result.unresolved.is_empty());
}
#[test]
fn nonempty_strings_remain_refused() {
    let (report, candidate) = plan();
    for bytes in [
        br#"{"value":"missing"}"#.as_slice(),
        br#"{"value":" "}"#,
        br#"{"value":"\u0020"}"#,
    ] {
        let result = run(
            bytes,
            &report,
            std::slice::from_ref(&candidate),
            Mode::Repair,
        );
        unchanged(&result, bytes);
        assert_eq!(
            result.assessments[0].status,
            Status::Rejected(Reject::Unsupported)
        );
    }
}
#[test]
fn number_to_null_remains_a_semantic_numeric_refusal() {
    let bytes = br#"{"value":0}"#;
    let (report, candidate) = plan();
    let result = run(bytes, &report, &[candidate], Mode::Repair);
    unchanged(&result, bytes);
    assert_eq!(
        result.assessments[0].status,
        Status::Rejected(Reject::NumericCoercion)
    );
    assert_eq!(result.class(), Some(DriftClass::Semantic));
}
#[test]
fn existing_null_is_a_borrowed_noop() {
    let bytes = br#"{"value":null}"#;
    let (report, candidate) = plan();
    let result = run(bytes, &report, &[candidate], Mode::Repair);
    unchanged(&result, bytes);
    assert_eq!(result.assessments[0].status, Status::Noop);
}
#[test]
fn empty_mapping_requires_exact_semantic_evidence_and_confidence() {
    let bytes = br#"{"value":""}"#;
    for mutation in 0..5 {
        let (mut report, mut candidate) = plan();
        if let Some(change) = report.changes.first_mut() {
            match mutation {
                0 => change.class = DriftClass::Shape,
                1 => change.class = DriftClass::Unknown,
                2 => change.kind = Kind::TypeChanged,
                3 => change.reason = Reason::DeclaredStructure,
                _ => candidate.evidence.detector = 0.989,
            }
        }
        unchanged(&run(bytes, &report, &[candidate], Mode::Repair), bytes);
    }
    let (report, mut candidate) = plan();
    candidate.evidence.detector = 0.99;
    assert_eq!(
        run(
            bytes,
            &report,
            std::slice::from_ref(&candidate),
            Mode::Repair
        )
        .assessments[0]
            .status,
        Status::Applied
    );
    unchanged(&run(bytes, &report, &[candidate], Mode::Observe), bytes);
}
#[test]
fn pooled_mapping_rejects_the_whole_plan_if_one_string_is_nonempty() {
    let bytes = br#"{"items":[{"value":""},{"value":"do not erase"}]}"#;
    let (mut report, candidate) = plan();
    if let Some(change) = report.changes.first_mut() {
        change.path = "items[].value".into();
    }
    unchanged(&run(bytes, &report, &[candidate], Mode::Repair), bytes);
}

#[test]
fn later_numeric_array_member_aborts_the_eligible_empty_string_patch() {
    let bytes = br#"{"items":[{"value":""},{"value":0}]}"#;
    let (mut report, candidate) = plan();
    if let Some(change) = report.changes.first_mut() {
        change.path = "items[].value".into();
    }
    let result = run(bytes, &report, &[candidate], Mode::Repair);
    unchanged(&result, bytes);
    assert_eq!(
        result.assessments[0].status,
        Status::Rejected(Reject::NumericCoercion)
    );
}
