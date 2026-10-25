use agw_diff::{Change, DriftClass as C, Kind, Reason, Side};
use agw_repair::{Candidate, Evidence, JsonType as T, Mode, Operation, Reject, Status, evaluate};

fn finding(class: C) -> Change {
    Change {
        side: Side::Request,
        path: "source".into(),
        to_path: Some("payment_method".into()),
        kind: Kind::Renamed,
        class,
        reason: Reason::ConfirmedIdentity,
    }
}
fn proposal(score: f64) -> Candidate {
    Candidate {
        finding: 0,
        operation: Operation::Rename {
            to: "payment_method".into(),
        },
        evidence: Evidence {
            detector: score,
            applicability: 1.0,
        },
    }
}

#[test]
fn confidence_is_the_minimum_and_shape_threshold_is_inclusive() {
    let change = finding(C::Shape);
    let mut candidate = proposal(0.99);
    candidate.evidence.applicability = 0.97;
    let assessment = evaluate(&change, &candidate, T::String, T::String, Mode::Repair);
    assert_eq!(assessment.confidence, Some(0.97));
    assert_eq!(assessment.status, Status::Eligible);
    candidate.evidence.applicability = 0.969999;
    assert_eq!(
        evaluate(&change, &candidate, T::String, T::String, Mode::Repair).status,
        Status::Rejected(Reject::BelowThreshold)
    );
}

#[test]
fn strict_and_semantic_thresholds_are_point_99() {
    let change = finding(C::Shape);
    for (score, expected) in [
        (0.989999, Status::Rejected(Reject::BelowThreshold)),
        (0.99, Status::Eligible),
    ] {
        assert_eq!(
            evaluate(
                &change,
                &proposal(score),
                T::String,
                T::String,
                Mode::Strict
            )
            .status,
            expected
        );
        let mut semantic = change.clone();
        semantic.class = C::Semantic;
        semantic.kind = Kind::UnitsChanged;
        let mut candidate = proposal(score);
        candidate.operation = Operation::Replace(b"42".to_vec());
        assert_eq!(
            evaluate(&semantic, &candidate, T::Number, T::Number, Mode::Repair).status,
            expected
        );
    }
}

#[test]
fn invalid_evidence_and_unknown_drift_never_authorize_repairs() {
    for score in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.1, 1.01] {
        for applicability in [false, true] {
            let mut candidate = proposal(1.0);
            if applicability {
                candidate.evidence.applicability = score;
            } else {
                candidate.evidence.detector = score;
            }
            assert_eq!(
                evaluate(
                    &finding(C::Shape),
                    &candidate,
                    T::String,
                    T::String,
                    Mode::Repair
                )
                .status,
                Status::Rejected(Reject::InvalidEvidence)
            );
        }
    }
    assert_eq!(
        evaluate(
            &finding(C::Unknown),
            &proposal(1.0),
            T::String,
            T::String,
            Mode::Repair
        )
        .status,
        Status::Rejected(Reject::UnknownDrift)
    );
}

#[test]
fn rename_requires_confirmed_identity_and_the_exact_sibling_destination() {
    let mut change = finding(C::Shape);
    change.reason = Reason::DeclaredStructure;
    assert_eq!(
        evaluate(&change, &proposal(1.0), T::String, T::String, Mode::Repair).status,
        Status::Rejected(Reject::UnconfirmedIdentity)
    );
    change.reason = Reason::ConfirmedIdentity;
    change.to_path = Some("another.payment_method".into());
    assert_eq!(
        evaluate(&change, &proposal(1.0), T::String, T::String, Mode::Repair).status,
        Status::Rejected(Reject::UnconfirmedIdentity)
    );
}

#[test]
fn numeric_coercion_overrides_a_wrong_class_and_low_confidence() {
    let mut change = finding(C::Shape);
    change.kind = Kind::TypeChanged;
    let mut candidate = proposal(0.1);
    candidate.operation = Operation::Replace(b"123".to_vec());
    let result = evaluate(&change, &candidate, T::String, T::Number, Mode::Repair);
    assert_eq!(result.class, C::Semantic);
    assert_eq!(result.status, Status::Rejected(Reject::NumericCoercion));
    candidate.operation = Operation::Replace(br#""id""#.to_vec());
    let reverse = evaluate(&change, &candidate, T::Number, T::String, Mode::Repair);
    assert_eq!(reverse.class, C::Semantic);
    assert_eq!(reverse.status, Status::Rejected(Reject::NumericCoercion));
}

#[test]
fn observe_assesses_without_authorizing_a_patch() {
    assert_eq!(
        evaluate(
            &finding(C::Shape),
            &proposal(1.0),
            T::String,
            T::String,
            Mode::Observe
        )
        .status,
        Status::Observed
    );
}

#[test]
fn nonnumeric_type_transitions_and_container_replacements_are_unsupported() {
    let mut candidate = proposal(1.0);
    candidate.operation = Operation::Replace(b"true".to_vec());
    let mut change = finding(C::Shape);
    change.kind = Kind::TypeChanged;
    for (from, to) in [
        (T::String, T::Bool),
        (T::Object, T::Object),
        (T::Array, T::Array),
    ] {
        assert_eq!(
            evaluate(&change, &candidate, from, to, Mode::Repair).status,
            Status::Rejected(Reject::Unsupported)
        );
    }
}

#[test]
fn root_or_missing_rename_destination_cannot_be_confirmed() {
    let mut change = finding(C::Shape);
    change.path = "$".into();
    change.to_path = None;
    assert_eq!(
        evaluate(&change, &proposal(1.0), T::Object, T::Object, Mode::Repair).status,
        Status::Rejected(Reject::UnconfirmedIdentity)
    );
}
