use agw_diff::{
    DriftClass as C, Evidence, Kind as K, Limits, Node, Reason as R, Side, Skip, Snapshot, compare,
    compare_with_limits,
};
use agw_inspector::inference::JsonType as T;

fn snap(node: Node) -> Snapshot {
    Snapshot {
        node,
        evidence: Evidence::Authoritative,
    }
}
fn diff(a: Node, b: Node) -> agw_diff::Report {
    compare(Side::Request, &snap(a), &snap(b))
}

#[test]
fn numeric_coercion_is_semantic_in_both_directions() {
    for n in [T::Integer, T::Number] {
        for other in [T::String, T::Boolean, T::Object, T::Array, T::Null] {
            for (a, b) in [(n, other), (other, n)] {
                let report = diff(Node::scalar(a), Node::scalar(b));
                assert_eq!(report.changes[0].reason, R::NumericCoercion);
                assert_eq!(report.class(), Some(C::Semantic));
            }
        }
    }
}

#[test]
fn integer_widening_is_compatible_but_narrowing_changes_shape() {
    assert!(
        diff(Node::scalar(T::Integer), Node::scalar(T::Number))
            .changes
            .is_empty()
    );
    assert_eq!(
        diff(Node::scalar(T::Number), Node::scalar(T::Integer)).class(),
        Some(C::Shape)
    );
}

#[test]
fn explicit_semantic_evidence_is_classified_with_reasons() {
    let mut a = Node::scalar(T::String);
    let mut b = a.clone();
    a.units = Some("cents".into());
    b.units = Some("dollars".into());
    a.enum_values = Some(["a".into()].into());
    b.enum_values = Some(["a".into(), "b".into()].into());
    a.null_meaning = Some("delete".into());
    b.null_meaning = Some("ignore".into());
    let report = diff(a, b);
    for (kind, reason) in [
        (K::UnitsChanged, R::UnitsChanged),
        (K::EnumExpanded, R::EnumExpanded),
        (K::NullMeaningChanged, R::MeaningChanged),
    ] {
        assert!(
            report
                .changes
                .iter()
                .any(|c| c.kind == kind && c.class == C::Semantic && c.reason == reason)
        );
    }
    assert_eq!(report.changes.len(), 3);
}

#[test]
fn nullability_change_is_semantic_without_numeric_coercion() {
    let a = Node::scalar(T::String);
    let mut b = a.clone();
    b.types.insert(T::Null);
    let report = diff(a, b);
    assert_eq!(report.changes[0].reason, R::NullabilityChanged);
    assert_eq!(report.class(), Some(C::Semantic));
}

#[test]
fn lost_metadata_and_enum_replacement_stay_unknown() {
    let mut a = Node::scalar(T::String);
    a.units = Some("meters".into());
    a.enum_values = Some(["a".into()].into());
    let mut b = Node::scalar(T::String);
    b.enum_values = Some(["b".into()].into());
    let report = diff(a, b);
    assert!(report.changes.iter().all(|c| c.class == C::Unknown));
    assert_eq!(report.class(), Some(C::Unknown));
}

#[test]
fn invalid_schema_and_each_limit_skip_without_vendor_drift() {
    let base = snap(Node::object([("id".into(), Node::scalar(T::String))]));
    for limits in [
        Limits {
            max_nodes: 1,
            ..Limits::default()
        },
        Limits {
            max_depth: 0,
            ..Limits::default()
        },
        Limits {
            max_text_bytes: 1,
            ..Limits::default()
        },
    ] {
        let report = compare_with_limits(Side::Request, &base, &base, limits);
        assert_eq!(report.skipped, Some(Skip::ResourceLimit));
        assert!(report.changes.is_empty());
    }
    let mut invalid = Node::scalar(T::String);
    invalid.required.insert("absent".into());
    assert_eq!(
        compare(Side::Request, &base, &snap(invalid)).skipped,
        Some(Skip::InvalidSchema)
    );
}

#[test]
fn requiredness_change_has_its_own_finding() {
    let a = Node::object([("id".into(), Node::scalar(T::String))]);
    let mut b = a.clone();
    b.required.insert("id".into());
    let report = diff(a, b);
    assert_eq!(report.changes[0].kind, K::RequiredChanged);
}

#[test]
fn numeric_widening_is_compatible_inside_unions_too() {
    let mut a = Node::scalar(T::Integer);
    a.types.insert(T::String);
    let mut b = Node::scalar(T::Number);
    b.types.insert(T::String);
    assert!(diff(a.clone(), b.clone()).changes.is_empty());
    assert_eq!(diff(b, a).class(), Some(C::Shape));
}
