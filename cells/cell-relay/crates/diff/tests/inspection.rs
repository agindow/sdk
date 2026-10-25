use agw_diff::{
    DriftClass, Evidence, Kind, Limits, Node, Side, Skip, Snapshot, compare_inspection,
    compare_inspection_with_limits,
};
use agw_inspector::{inference::JsonType as T, inspect};

fn baseline() -> Snapshot {
    Snapshot {
        node: Node::object([("id".into(), Node::scalar(T::String))]),
        evidence: Evidence::Authoritative,
    }
}

#[test]
fn complete_malformed_body_is_one_shape_finding_on_both_sides() {
    for bytes in [b"{".as_slice(), b"", b"[]", b"null", b"123"] {
        for side in [Side::Request, Side::Response] {
            let report = compare_inspection(side, &baseline(), &inspect(bytes, false, None));
            assert_eq!(report.skipped, None);
            assert_eq!(report.changes.len(), 1);
            let c = &report.changes[0];
            assert_eq!(
                (c.path.as_str(), c.side, c.kind, c.class),
                ("$", side, Kind::MalformedBody, DriftClass::Shape)
            );
        }
    }
}

#[test]
fn truncated_capture_never_becomes_a_drift_finding_even_if_it_parses() {
    for bytes in [b"{".as_slice(), b"{\"id\":3}"] {
        let report = compare_inspection(Side::Response, &baseline(), &inspect(bytes, true, None));
        assert!(report.changes.is_empty());
        assert_eq!(report.skipped, Some(Skip::Incomplete));
    }
}

#[test]
fn duplicate_keys_are_not_silently_collapsed() {
    let report = compare_inspection(
        Side::Response,
        &baseline(),
        &inspect(br#"{"id":"a","id":12}"#, false, None),
    );
    assert_eq!(report.skipped, Some(Skip::AmbiguousObject));
    assert!(report.changes.is_empty());
}

#[test]
fn observed_numeric_id_type_change_is_semantic_and_inputs_are_unchanged() {
    let baseline = baseline();
    let saved = baseline.clone();
    let inspection = inspect(br#"{"id":18446744073709551616}"#, false, None);
    let original = inspection.clone();
    let report = compare_inspection(Side::Request, &baseline, &inspection);
    assert_eq!(report.class(), Some(DriftClass::Semantic));
    assert_eq!(report.changes[0].path, "id");
    assert_eq!(baseline, saved);
    assert_eq!(inspection, original);
}

#[test]
fn compatible_observed_union_and_empty_arrays_do_not_report_type_drift() {
    let mut number = Node::scalar(T::Number);
    number.types.insert(T::String);
    let base = Snapshot {
        node: Node::array(number),
        evidence: Evidence::Authoritative,
    };
    for bytes in [b"[]".as_slice(), b"[1,1e400,\"x\"]"] {
        assert!(
            compare_inspection(Side::Response, &base, &inspect(bytes, false, None))
                .changes
                .is_empty()
        );
    }
}

#[test]
fn limits_skip_without_partial_findings() {
    let parsed = inspect(br#"{"id":"a","extra":[1,2,3]}"#, false, None);
    let report = compare_inspection_with_limits(
        Side::Response,
        &baseline(),
        &parsed,
        Limits {
            max_nodes: 4,
            ..Limits::default()
        },
    );
    assert_eq!(report.skipped, Some(Skip::ResourceLimit));
    assert!(report.changes.is_empty());
}

#[test]
fn populated_array_with_no_baseline_item_evidence_is_unknown() {
    let baseline = Snapshot {
        node: Node::scalar(T::Array),
        evidence: Evidence::Learned,
    };
    let report = compare_inspection(Side::Response, &baseline, &inspect(b"[true]", false, None));
    assert_eq!(report.class(), Some(DriftClass::Unknown));
    assert_eq!(report.changes[0].path, "$[]");
}

#[test]
fn parser_resource_and_io_failures_are_not_vendor_drift() {
    for class in [
        agw_inspector::ParseClass::Data,
        agw_inspector::ParseClass::Io,
    ] {
        let inspection = agw_inspector::Inspection {
            parse: agw_inspector::Parse::Unparseable(agw_inspector::ParseFailure {
                class,
                line: 0,
                column: 0,
                message: "internal limit or I/O".into(),
            }),
            truncated: false,
        };
        let report = compare_inspection(Side::Response, &baseline(), &inspection);
        assert_eq!(report.skipped, Some(Skip::ParserFailure));
        assert!(report.changes.is_empty());
    }
}
