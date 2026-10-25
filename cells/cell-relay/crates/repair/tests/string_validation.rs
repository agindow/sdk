//! TI-44: replacement strings must be accepted by the inspection parser.
mod support;
use agw_diff::DriftClass;
use agw_inspector::{Parse, inspect};
use agw_repair::{Mode, Reject, Status};
use support::{rename, replacement, run, unchanged};

#[test]
fn unpaired_surrogate_replacements_are_rejected() {
    let bytes = br#"{"source":"a"}"#;
    for token in [br#""\uD800""#.as_slice(), br#""\uDC00""#, br#""\uD800x""#] {
        let (report, candidate) = replacement("source", token, DriftClass::Shape);
        let outcome = run(bytes, &report, &[candidate], Mode::Repair);
        unchanged(&outcome, bytes);
        assert_eq!(
            outcome.assessments[0].status,
            Status::Rejected(Reject::InvalidReplacement)
        );
        assert_eq!(outcome.unresolved, report.changes);
    }
}

#[test]
fn invalid_string_replacement_aborts_other_eligible_proposals() {
    let bytes = br#"{"source":"a","s":"b"}"#;
    let (mut report, first) = rename("source", "target", "target");
    let (other, mut invalid) = replacement("s", br#""\uD800""#, DriftClass::Shape);
    invalid.finding = 1;
    report.changes.extend(other.changes);
    let outcome = run(bytes, &report, &[first, invalid], Mode::Repair);
    unchanged(&outcome, bytes);
    assert_eq!(outcome.assessments[0].status, Status::NotApplied);
    assert_eq!(
        outcome.assessments[1].status,
        Status::Rejected(Reject::InvalidReplacement)
    );
    assert_eq!(outcome.unresolved, report.changes);
}

#[test]
fn valid_surrogate_pairs_and_escapes_preserve_replacement_bytes() {
    let bytes = br#"{ "s" : "a", "n" : 1e400 }"#;
    let (report, candidate) = replacement("s", br#""\uD83D\uDE00\n\"""#, DriftClass::Shape);
    let outcome = run(bytes, &report, &[candidate], Mode::Repair);
    assert_eq!(outcome.assessments[0].status, Status::Applied);
    assert_eq!(
        outcome.body.as_ref(),
        br#"{ "s" : "\uD83D\uDE00\n\"", "n" : 1e400 }"#
    );
    assert!(matches!(
        inspect(&outcome.body, false, None).parse,
        Parse::Parsed(_)
    ));
}
