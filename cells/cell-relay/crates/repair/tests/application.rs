mod support;
use agw_diff::{DriftClass, Side};
use agw_repair::{Mode, Reject, Status};
use support::{rename, replacement, run, unchanged};

#[test]
fn renames_one_field_beside_14_or_99_others_without_reserializing() {
    for count in [14, 99] {
        let tail = (0..count)
            .map(|i| format!(", \"field_{i}\" : [18446744073709551616, 1e400, -0, \"\\u0061\"]"))
            .collect::<String>();
        let bytes = format!(" \n{{ \"so\\u0075rce\" : \"tok_123\"{tail} }} \t").into_bytes();
        let expected = format!(" \n{{ \"payment_method\" : \"tok_123\"{tail} }} \t").into_bytes();
        let (report, candidate) = rename("source", "payment_method", "payment_method");
        let outcome = run(&bytes, &report, &[candidate], Mode::Repair);
        assert_eq!(outcome.body.as_ref(), expected);
        let patch = outcome
            .patch()
            .unwrap_or_else(|| panic!("no applied patch"));
        assert!(patch.applied);
        assert!(!patch.byte_identical);
        assert_eq!(patch.selective, Some(true));
        assert_eq!(
            serde_json::to_value(&patch.fields_changed).ok(),
            Some(serde_json::json!(["source"]))
        );
        assert!(outcome.unresolved.is_empty());
        assert_eq!(outcome.assessments[0].status, Status::Applied);
    }
}

#[test]
fn scalar_numeric_replacement_is_exact_and_preserves_unrelated_lexemes() {
    let bytes = br#"{ "amount" : 9007199254740993, "id":18446744073709551616, "n":1e400 }"#;
    let (report, candidate) = replacement("amount", b"90071992547409.93", DriftClass::Semantic);
    let outcome = run(bytes, &report, &[candidate], Mode::Repair);
    assert_eq!(
        outcome.body.as_ref(),
        br#"{ "amount" : 90071992547409.93, "id":18446744073709551616, "n":1e400 }"#
    );
    assert_eq!(outcome.confidence(), Some(1.0));
}

#[test]
fn pooled_array_renames_record_concrete_paths_and_skip_absent_optional_fields() {
    let bytes = br#"{"items":[{"source":"a"}, {}, {"source":"b", "id":1e400}]}"#;
    let (report, candidate) = rename("items[].source", "items[].payment_method", "payment_method");
    let outcome = run(bytes, &report, &[candidate], Mode::Repair);
    assert_eq!(
        outcome.body.as_ref(),
        br#"{"items":[{"payment_method":"a"}, {}, {"payment_method":"b", "id":1e400}]}"#
    );
    let patch = outcome.patch().unwrap_or_else(|| panic!("missing patch"));
    assert_eq!(
        serde_json::to_value(&patch.fields_changed).ok(),
        Some(serde_json::json!(["items[0].source", "items[2].source"]))
    );
}

#[test]
fn low_confidence_and_observe_return_original_borrowed_bytes() {
    let bytes = br#"{"source":"a"}"#;
    let (report, mut candidate) = rename("source", "payment_method", "payment_method");
    candidate.evidence.detector = 0.96;
    let outcome = run(
        bytes,
        &report,
        std::slice::from_ref(&candidate),
        Mode::Repair,
    );
    unchanged(&outcome, bytes);
    assert_eq!(outcome.class(), Some(DriftClass::Shape));
    assert_eq!(
        outcome.assessments[0].status,
        Status::Rejected(Reject::BelowThreshold)
    );
    candidate.evidence.detector = 1.0;
    let observed = run(bytes, &report, &[candidate], Mode::Observe);
    unchanged(&observed, bytes);
    assert_eq!(observed.assessments[0].status, Status::Observed);
}

#[test]
fn numeric_coercion_is_refused_and_handed_to_policy_as_semantic() {
    let bytes = br#"{"id":"123"}"#;
    let (report, mut candidate) = replacement("id", b"123", DriftClass::Shape);
    candidate.evidence.detector = 0.01;
    let outcome = run(bytes, &report, &[candidate], Mode::Repair);
    unchanged(&outcome, bytes);
    assert_eq!(outcome.class(), Some(DriftClass::Semantic));
    assert_eq!(
        outcome.assessments[0].status,
        Status::Rejected(Reject::NumericCoercion)
    );
}

#[test]
fn responses_use_the_same_patcher_and_retain_side() {
    let bytes = br#"{"source":"a"}"#;
    let (mut report, candidate) = rename("source", "payment_method", "payment_method");
    report.changes[0].side = Side::Response;
    let result = agw_repair::repair(
        Side::Response,
        bytes,
        &agw_inspector::inspect(bytes, false, None),
        &report,
        &[candidate],
        Mode::Repair,
    );
    assert_eq!(result.body.as_ref(), br#"{"payment_method":"a"}"#);
}
