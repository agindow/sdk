use agw_diff::{DriftClass, Evidence, Kind, Node, Reason, Side, Snapshot, compare};
use agw_inspector::inference::JsonType as T;

fn snapshot(node: Node) -> Snapshot {
    Snapshot {
        node,
        evidence: Evidence::Authoritative,
    }
}
fn object(fields: &[(&str, T)]) -> Node {
    Node::object(
        fields
            .iter()
            .map(|(k, t)| (k.to_string(), Node::scalar(*t))),
    )
}

#[test]
fn structural_changes_have_paths_sides_classes_and_reasons() {
    let before = snapshot(object(&[("removed", T::String), ("changed", T::Boolean)]));
    let mut after = object(&[
        ("optional", T::String),
        ("required", T::String),
        ("changed", T::Object),
    ]);
    after.required.insert("required".into());
    let after = snapshot(after);
    for side in [Side::Request, Side::Response] {
        let report = compare(side, &before, &after);
        assert_eq!(report.skipped, None);
        for (path, kind, reason) in [
            ("removed", Kind::Removed, Reason::DeclaredStructure),
            ("changed", Kind::TypeChanged, Reason::DeclaredStructure),
            ("optional", Kind::OptionalAdded, Reason::OptionalInformation),
            ("required", Kind::RequiredAdded, Reason::DeclaredStructure),
        ] {
            let found = report.changes.iter().find(|c| c.path == path);
            assert_eq!(
                found.map(|c| (c.side, c.kind, c.class, c.reason)),
                Some((side, kind, DriftClass::Shape, reason))
            );
        }
        assert_eq!(report.changes.len(), 4);
    }
}

#[test]
fn learned_presence_does_not_prove_removal_or_requiredness() {
    let before = Snapshot {
        node: object(&[("old", T::String)]),
        evidence: Evidence::Learned,
    };
    let after = snapshot(object(&[("new", T::String)]));
    let report = compare(Side::Request, &before, &after);
    assert_eq!(report.changes.len(), 2);
    assert!(
        report
            .changes
            .iter()
            .all(|c| c.class == DriftClass::Unknown && c.reason == Reason::InsufficientEvidence)
    );
    assert_eq!(report.class(), Some(DriftClass::Unknown));
}

#[test]
fn confirmed_identity_pairs_rename_but_still_compares_meaning() {
    let mut old = Node::scalar(T::String);
    old.identity = Some("payment-token".into());
    let mut new = Node::scalar(T::Integer);
    new.identity = old.identity.clone();
    let before = snapshot(Node::object([("source".into(), old)]));
    let after = snapshot(Node::object([("payment_method".into(), new)]));
    let report = compare(Side::Request, &before, &after);
    assert!(report.changes.iter().any(|c| c.kind == Kind::Renamed
        && c.path == "source"
        && c.to_path.as_deref() == Some("payment_method")));
    assert!(
        report
            .changes
            .iter()
            .any(|c| c.path == "payment_method" && c.class == DriftClass::Semantic)
    );
    assert_eq!(report.class(), Some(DriftClass::Semantic));
}

#[test]
fn ambiguous_identity_never_confirms_a_rename() {
    let mut field = Node::scalar(T::String);
    field.identity = Some("shared".into());
    let before = snapshot(Node::object([("a".into(), field.clone())]));
    let after = snapshot(Node::object([
        ("b".into(), field.clone()),
        ("c".into(), field),
    ]));
    let report = compare(Side::Request, &before, &after);
    assert_eq!(report.changes.len(), 3);
    assert!(
        report
            .changes
            .iter()
            .all(|c| c.kind != Kind::Renamed && c.class == DriftClass::Unknown)
    );
}

#[test]
fn nested_arrays_escape_literal_keys_and_empty_names() {
    let before = snapshot(Node::array(object(&[
        ("a.b[]$\\", T::String),
        ("", T::String),
    ])));
    let after = snapshot(Node::array(object(&[])));
    let report = compare(Side::Response, &before, &after);
    let paths: Vec<_> = report.changes.iter().map(|c| c.path.as_str()).collect();
    assert_eq!(paths, ["$[].\\e", "$[].a\\.b\\[\\]\\$\\\\"]);
}
