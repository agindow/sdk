use std::{collections::BTreeMap, num::NonZeroUsize};

use agw_diff::{DriftClass, Evidence, Kind, Node, Side, Snapshot, compare};
use agw_inspector::inference::{BaselineStore, JsonType as T, Key, Limits};

#[test]
fn learned_schema_adapter_does_not_promote_presence_confidence_to_authority()
-> Result<(), Box<dyn std::error::Error>> {
    let mut store = BaselineStore::new(
        NonZeroUsize::new(1).ok_or("zero samples")?,
        Limits::default(),
    );
    let key = Key::new("vendor", "operation");
    store.begin(key.clone(), None)?;
    store.observe(&key, br#"{"id":"a"}"#, false)?;
    let baseline = store.latest(&key).ok_or("no learned baseline")?;
    assert_eq!(baseline.schema.properties["id"].confidence, 1.0);
    let before = Snapshot::from(&baseline.schema);
    assert_eq!(before.evidence, Evidence::Learned);
    assert!(before.node.required.is_empty());
    assert_eq!(before.node.types, baseline.schema.types);
    assert_eq!(before.node.properties.len(), 1);
    let after = Snapshot {
        node: Node::object(BTreeMap::new()),
        evidence: Evidence::Authoritative,
    };
    assert_eq!(
        compare(Side::Request, &before, &after).class(),
        Some(DriftClass::Unknown)
    );
    Ok(())
}

#[test]
fn name_similarity_and_equal_types_do_not_confirm_identity() {
    let before = Snapshot {
        node: Node::object([("source".into(), Node::scalar(T::String))]),
        evidence: Evidence::Authoritative,
    };
    let after = Snapshot {
        node: Node::object([("source_new".into(), Node::scalar(T::String))]),
        evidence: Evidence::Authoritative,
    };
    let report = compare(Side::Request, &before, &after);
    assert_eq!(report.changes.len(), 2);
    assert!(report.changes.iter().all(|c| c.kind != Kind::Renamed));
}

#[test]
fn unchanged_sibling_with_the_same_identity_makes_rename_ambiguous() {
    let mut field = Node::scalar(T::String);
    field.identity = Some("id".into());
    let before = Snapshot {
        node: Node::object([
            ("old".into(), field.clone()),
            ("retained".into(), field.clone()),
        ]),
        evidence: Evidence::Authoritative,
    };
    let after = Snapshot {
        node: Node::object([("new".into(), field.clone()), ("retained".into(), field)]),
        evidence: Evidence::Authoritative,
    };
    let report = compare(Side::Request, &before, &after);
    assert_eq!(report.changes.len(), 2);
    assert!(
        report
            .changes
            .iter()
            .all(|c| c.class == DriftClass::Unknown && c.kind != Kind::Renamed)
    );
}

#[test]
fn comparing_a_snapshot_to_itself_reports_nothing_and_does_not_mutate_it() {
    let mut value = Node::array(Node::object([("key".into(), Node::scalar(T::String))]));
    value.types.insert(T::Null);
    let snapshot = Snapshot {
        node: value,
        evidence: Evidence::Authoritative,
    };
    let saved = snapshot.clone();
    for side in [Side::Request, Side::Response] {
        let report = compare(side, &snapshot, &snapshot);
        assert!(report.changes.is_empty());
        assert_eq!(report.class(), None);
        assert_eq!(report.skipped, None);
    }
    assert_eq!(snapshot, saved);
}
