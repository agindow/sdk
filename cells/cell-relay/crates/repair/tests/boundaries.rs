mod support;
use agw_diff::{DriftClass, Reason, Side};
use agw_inspector::inspect;
use agw_repair::{Failure, Limits, Mode, Reject, Status, repair, repair_with_limits};
use support::{rename, replacement, run, unchanged};

#[test]
fn truncated_malformed_duplicate_and_stale_captures_never_repair() {
    let (report, candidate) = rename("source", "target", "target");
    for (bytes, inspection, failure) in [
        (
            br#"{"source":"a"}"#.as_slice(),
            inspect(br#"{"source":"a"}"#, true, None),
            Failure::Incomplete,
        ),
        (
            b"{".as_slice(),
            inspect(b"{", false, None),
            Failure::Unparseable,
        ),
        (
            br#"{"source":1,"so\u0075rce":2}"#.as_slice(),
            inspect(br#"{"source":1,"so\u0075rce":2}"#, false, None),
            Failure::DuplicateKeys,
        ),
        (
            br#"{"source":"b"}"#.as_slice(),
            inspect(br#"{"source":"a"}"#, false, None),
            Failure::InspectionMismatch,
        ),
        (
            br#"{"source":1e400}"#.as_slice(),
            inspect(br#"{"source":2e400}"#, false, None),
            Failure::InspectionMismatch,
        ),
    ] {
        let result = repair(
            Side::Request,
            bytes,
            &inspection,
            &report,
            std::slice::from_ref(&candidate),
            Mode::Repair,
        );
        unchanged(&result, bytes);
        assert_eq!(result.failure, Some(failure));
    }
}

#[test]
fn collision_and_missing_target_are_refused() {
    for (bytes, expected) in [
        (br#"{"source":1,"target":2}"#.as_slice(), Reject::Collision),
        (br#"{"other":1}"#.as_slice(), Reject::MissingTarget),
    ] {
        let (report, candidate) = rename("source", "target", "target");
        let result = run(bytes, &report, &[candidate], Mode::Repair);
        unchanged(&result, bytes);
        assert_eq!(result.assessments[0].status, Status::Rejected(expected));
    }
}

#[test]
fn one_refusal_aborts_the_whole_supplied_plan() {
    let bytes = br#"{"source":"a","id":"123"}"#;
    let (mut report, rename) = rename("source", "target", "target");
    let (coercion, mut replacement) = replacement("id", b"123", DriftClass::Shape);
    report.changes.extend(coercion.changes);
    replacement.finding = 1;
    let result = run(bytes, &report, &[rename, replacement], Mode::Repair);
    unchanged(&result, bytes);
    assert_eq!(result.unresolved.len(), 2);
    assert_eq!(result.class(), Some(DriftClass::Semantic));
    assert_eq!(result.assessments[0].status, Status::NotApplied);
}

#[test]
fn duplicate_edits_and_two_renames_to_one_destination_are_rejected() {
    let bytes = br#"{"a":1,"b":2}"#;
    let (mut report, first) = rename("a", "target", "target");
    let (other, mut second) = rename("b", "target", "target");
    second.finding = 1;
    report.changes.extend(other.changes);
    let collision = run(bytes, &report, &[first.clone(), second], Mode::Repair);
    unchanged(&collision, bytes);
    assert!(
        collision
            .assessments
            .iter()
            .any(|a| a.status == Status::Rejected(Reject::Collision))
    );
    let overlap = run(bytes, &report, &[first.clone(), first], Mode::Repair);
    unchanged(&overlap, bytes);
    assert!(
        overlap
            .assessments
            .iter()
            .all(|a| a.status == Status::Rejected(Reject::Overlap))
    );
}

#[test]
fn all_limits_return_borrowed_bytes_without_partial_application() {
    let bytes = br#"{"source":"a"}"#;
    let (report, candidate) = rename(
        "source",
        "a_very_long_replacement_key",
        "a_very_long_replacement_key",
    );
    for limits in [
        Limits {
            max_bytes: bytes.len() - 1,
            ..Limits::default()
        },
        Limits {
            max_depth: 0,
            ..Limits::default()
        },
        Limits {
            max_nodes: 1,
            ..Limits::default()
        },
        Limits {
            max_proposals: 0,
            ..Limits::default()
        },
    ] {
        let result = repair_with_limits(
            Side::Request,
            bytes,
            &inspect(bytes, false, None),
            &report,
            std::slice::from_ref(&candidate),
            Mode::Repair,
            limits,
        );
        unchanged(&result, bytes);
        assert_eq!(result.failure, Some(Failure::ResourceLimit));
    }
    // Input/index/metadata fit, but pooled expansion exceeds the output cap.
    let bytes = br#"{"items":["a","a","a","a","a"]}"#;
    let (report, candidate) = replacement(
        "items[]",
        br#""abcdefghijklmnopqrstuvwxyz""#,
        DriftClass::Shape,
    );
    let result = repair_with_limits(
        Side::Request,
        bytes,
        &inspect(bytes, false, None),
        &report,
        &[candidate],
        Mode::Repair,
        Limits {
            max_bytes: 100,
            ..Limits::default()
        },
    );
    unchanged(&result, bytes);
    assert_eq!(result.failure, Some(Failure::ResourceLimit));
}

#[test]
fn unconfirmed_unknown_invalid_findings_and_wrong_side_do_not_repair() {
    let bytes = br#"{"source":1}"#;
    let (mut report, mut candidate) = rename("source", "target", "target");
    report.changes[0].reason = Reason::InsufficientEvidence;
    let result = run(
        bytes,
        &report,
        std::slice::from_ref(&candidate),
        Mode::Repair,
    );
    unchanged(&result, bytes);
    assert_eq!(
        result.assessments[0].status,
        Status::Rejected(Reject::UnconfirmedIdentity)
    );
    report.changes[0].class = DriftClass::Unknown;
    let result = run(
        bytes,
        &report,
        std::slice::from_ref(&candidate),
        Mode::Repair,
    );
    unchanged(&result, bytes);
    assert_eq!(
        result.assessments[0].status,
        Status::Rejected(Reject::UnknownDrift)
    );
    candidate.finding = 99;
    unchanged(
        &run(
            bytes,
            &report,
            std::slice::from_ref(&candidate),
            Mode::Repair,
        ),
        bytes,
    );
    report.changes[0].side = Side::Response;
    assert_eq!(
        run(bytes, &report, &[candidate], Mode::Repair).failure,
        Some(Failure::InvalidReport)
    );
}

#[test]
fn heterogeneous_array_numeric_coercion_is_not_hidden_by_an_earlier_refusal() {
    let bytes = br#"{"items":["a",123]}"#;
    let (report, mut candidate) = replacement("items[]", br#""b""#, DriftClass::Shape);
    candidate.evidence.detector = 0.1;
    let result = run(bytes, &report, &[candidate], Mode::Repair);
    unchanged(&result, bytes);
    assert_eq!(result.class(), Some(DriftClass::Semantic));
    assert_eq!(
        result.assessments[0].status,
        Status::Rejected(Reject::NumericCoercion)
    );
}

#[test]
fn unproposed_drift_is_retained_for_policy_after_an_applied_repair() {
    let bytes = br#"{"source":"a","amount":1}"#;
    let (mut report, candidate) = rename("source", "target", "target");
    let (unhandled, _) = replacement("amount", b"2", DriftClass::Semantic);
    report.changes.extend(unhandled.changes);
    let result = run(bytes, &report, &[candidate], Mode::Repair);
    assert!(result.patch().is_some());
    assert_eq!(result.class(), Some(DriftClass::Semantic));
    assert_eq!(result.unresolved.len(), 1);
    assert_eq!(result.unresolved[0].path, "amount");
}

#[test]
fn invalid_replacement_tokens_never_reencode_or_repair_a_body() {
    let bytes = br#"{"source":"a"}"#;
    for replacement_bytes in [b"".as_slice(), b"123 456", b" 123", b"123 ", b"[", b"NaN"] {
        let (report, candidate) = replacement("source", replacement_bytes, DriftClass::Shape);
        let result = run(bytes, &report, &[candidate], Mode::Repair);
        unchanged(&result, bytes);
        assert_eq!(
            result.assessments[0].status,
            Status::Rejected(Reject::InvalidReplacement)
        );
    }
}

#[test]
fn collision_on_a_later_array_element_aborts_all_earlier_staged_renames() {
    let bytes = br#"{"items":[{"source":1},{"source":2,"target":3}]}"#;
    let (report, candidate) = rename("items[].source", "items[].target", "target");
    let result = run(bytes, &report, &[candidate], Mode::Repair);
    unchanged(&result, bytes);
    assert_eq!(
        result.assessments[0].status,
        Status::Rejected(Reject::Collision)
    );
}
