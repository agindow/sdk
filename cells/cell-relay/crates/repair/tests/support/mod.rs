#![allow(dead_code)]
use agw_diff::{Change, DriftClass, Kind, Reason, Report, Side};
use agw_repair::{Candidate, Evidence, Mode, Operation, Outcome, repair};

pub fn rename(path: &str, destination: &str, to_key: &str) -> (Report, Candidate) {
    (
        Report {
            changes: vec![Change {
                side: Side::Request,
                path: path.into(),
                to_path: Some(destination.into()),
                kind: Kind::Renamed,
                class: DriftClass::Shape,
                reason: Reason::ConfirmedIdentity,
            }],
            skipped: None,
        },
        Candidate {
            finding: 0,
            operation: Operation::Rename { to: to_key.into() },
            evidence: evidence(1.0),
        },
    )
}
pub fn replacement(path: &str, bytes: &[u8], class: DriftClass) -> (Report, Candidate) {
    (
        Report {
            changes: vec![Change {
                side: Side::Request,
                path: path.into(),
                to_path: None,
                kind: agw_diff::Kind::UnitsChanged,
                class,
                reason: agw_diff::Reason::UnitsChanged,
            }],
            skipped: None,
        },
        Candidate {
            finding: 0,
            operation: Operation::Replace(bytes.into()),
            evidence: evidence(1.0),
        },
    )
}
pub fn evidence(score: f64) -> Evidence {
    Evidence {
        detector: score,
        applicability: 1.0,
    }
}
pub fn run<'a>(
    bytes: &'a [u8],
    report: &Report,
    candidates: &[Candidate],
    mode: Mode,
) -> Outcome<'a> {
    repair(
        Side::Request,
        bytes,
        &agw_inspector::inspect(bytes, false, None),
        report,
        candidates,
        mode,
    )
}
pub fn unchanged(result: &Outcome<'_>, original: &[u8]) {
    assert!(matches!(result.body, std::borrow::Cow::Borrowed(_)));
    assert!(std::ptr::eq(result.body.as_ref(), original));
    assert!(result.patch().is_none());
}
