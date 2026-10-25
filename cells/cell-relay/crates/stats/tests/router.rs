use agw_diff::{Change, DriftClass, Kind, Reason, Report, Side, Skip};
use agw_stats::router::{Error, Finding, Handler, Invariant, Label, route};

fn change(class: DriftClass) -> Change {
    Change {
        side: Side::Request,
        path: "amount".into(),
        to_path: None,
        kind: Kind::TypeChanged,
        class,
        reason: Reason::NumericCoercion,
    }
}

#[test]
fn mixed_findings_keep_identity_and_select_all_cheapest_handlers() -> Result<(), Error> {
    let report = Report {
        changes: vec![
            change(DriftClass::Shape),
            change(DriftClass::Semantic),
            change(DriftClass::Unknown),
        ],
        skipped: None,
    };
    let invariants = [Invariant {
        side: Side::Response,
        path: "total",
        rule: "sum(items)=total",
    }];
    let r = route(&report, &invariants)?;
    assert_eq!(r.unavailable(), None);
    assert_eq!(
        r.findings().iter().map(|d| d.handler()).collect::<Vec<_>>(),
        [
            Handler::Repair,
            Handler::Policy,
            Handler::Conservative,
            Handler::Verify
        ]
    );
    for (dispatch, original) in r.findings().iter().zip(&report.changes) {
        assert!(matches!(dispatch.finding(), Finding::Diff(c) if std::ptr::eq(c, original)));
    }
    assert!(
        matches!(r.findings()[3].finding(), Finding::Invariant(i) if std::ptr::eq(i, &invariants[0]))
    );
    assert_eq!(r.findings()[3].label(), Label::Invariant);
    Ok(())
}

#[test]
fn real_diff_output_routes_without_reclassifying_evidence() -> Result<(), Error> {
    use agw_diff::{Evidence, Node, Snapshot, compare};
    use agw_inspector::inference::JsonType;
    let before = Snapshot {
        node: Node::scalar(JsonType::Boolean),
        evidence: Evidence::Authoritative,
    };
    let after = Snapshot {
        node: Node::scalar(JsonType::String),
        evidence: Evidence::Authoritative,
    };
    let report = compare(Side::Response, &before, &after);
    let r = route(&report, &[])?;
    assert!(!r.findings().is_empty());
    for dispatch in r.findings() {
        assert!(matches!(dispatch.finding(), Finding::Diff(c) if c.side == Side::Response));
        assert_eq!(dispatch.label(), Label::Shape);
        assert_eq!(dispatch.handler(), Handler::Repair);
    }
    Ok(())
}

#[test]
fn clean_and_each_skip_are_distinct_and_schedule_no_work() -> Result<(), Error> {
    let clean = Report::default();
    let r = route(&clean, &[])?;
    assert!(r.findings().is_empty());
    assert_eq!(r.unavailable(), None);
    for skip in [
        Skip::Incomplete,
        Skip::ParserFailure,
        Skip::AmbiguousObject,
        Skip::InvalidSchema,
        Skip::ResourceLimit,
    ] {
        let report = Report {
            changes: vec![change(DriftClass::Unknown)],
            skipped: Some(skip),
        };
        let r = route(&report, &[])?;
        assert_eq!(r.unavailable(), Some(skip));
        assert!(r.findings().is_empty());
    }
    Ok(())
}

#[test]
fn findings_and_all_text_are_bounded_before_routing() -> Result<(), Error> {
    let mut report = Report {
        changes: vec![change(DriftClass::Shape); 4096],
        skipped: None,
    };
    assert_eq!(route(&report, &[])?.findings().len(), 4096);
    let invariants = [Invariant {
        side: Side::Request,
        path: "x",
        rule: "x>0",
    }];
    assert_eq!(
        route(&report, &invariants).err(),
        Some(Error::ResourceLimit)
    );
    report.changes.truncate(1);
    report.changes[0].path = "p".repeat(4097);
    assert_eq!(route(&report, &[]).err(), Some(Error::ResourceLimit));
    report.changes[0].path = "p".repeat(4096);
    assert!(route(&report, &[]).is_ok());
    report.changes[0].to_path = Some("x".repeat(4097));
    assert_eq!(route(&report, &[]).err(), Some(Error::ResourceLimit));
    report.changes[0].to_path = None;
    report.changes[0].path.clear();
    assert_eq!(route(&report, &[]).err(), Some(Error::InvalidInput));
    report.changes = vec![change(DriftClass::Unknown); 257];
    for c in &mut report.changes {
        c.path = "x".repeat(4096);
    }
    assert_eq!(route(&report, &[]).err(), Some(Error::ResourceLimit));
    report.changes.pop();
    assert!(route(&report, &[]).is_ok());
    assert_eq!(
        route(
            &Report::default(),
            &[Invariant {
                side: Side::Request,
                path: "x",
                rule: ""
            }]
        )
        .err(),
        Some(Error::InvalidInput)
    );
    Ok(())
}

#[test]
fn real_numeric_type_change_keeps_diff_semantic_fence() -> Result<(), Error> {
    use agw_diff::{Evidence, Node, Snapshot, compare};
    use agw_inspector::inference::JsonType;
    let before = Snapshot {
        node: Node::scalar(JsonType::Number),
        evidence: Evidence::Authoritative,
    };
    let after = Snapshot {
        node: Node::scalar(JsonType::String),
        evidence: Evidence::Authoritative,
    };
    let report = compare(Side::Request, &before, &after);
    let routing = route(&report, &[])?;
    assert_eq!(routing.findings().len(), 1);
    assert_eq!(routing.findings()[0].handler(), Handler::Policy);
    assert!(
        matches!(routing.findings()[0].finding(), Finding::Diff(c) if c.reason == Reason::NumericCoercion)
    );
    Ok(())
}
