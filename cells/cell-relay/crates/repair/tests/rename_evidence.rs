mod support;
use agw_diff::rename::{Decision, Field, match_fields};
use agw_diff::{DriftClass, Evidence as SchemaEvidence, Kind, Node, Side, Snapshot, compare};
use agw_inspector::inference::JsonType as T;
use agw_repair::{Candidate, Evidence, Mode, Operation, Reject, Status};

#[test]
fn distribution_evidence_handoff_preserves_repair_and_semantic_gates()
-> Result<(), Box<dyn std::error::Error>> {
    for samples in [2, 200] {
        let field = |name: &str| Field {
            name: name.into(),
            kind: T::String,
            binning: "exact".into(),
            counts: [("card".into(), samples)].into(),
            opportunities: samples,
        };
        let decisions = match_fields(&[field("source")], &[field("payment_method")])?;
        let Decision::Confirmed {
            added, confidence, ..
        } = decisions[0]
        else {
            return Err("missing identity".into());
        };
        assert_eq!(added, 0);
        // Caller assigns a scope-unique identity; distribution code never edits schemas.
        let mut old = Node::scalar(T::String);
        old.identity = Some("window-7/pair-0".into());
        let new = old.clone();
        let snapshot = |name: &str, node| Snapshot {
            node: Node::object([(name.into(), node)]),
            evidence: SchemaEvidence::Authoritative,
        };
        let before = snapshot("source", old);
        let after = snapshot("payment_method", new);
        let report = compare(Side::Request, &before, &after);
        assert_eq!(report.changes.len(), 1);
        assert_eq!(report.changes[0].kind, Kind::Renamed);
        let candidate = Candidate {
            finding: 0,
            operation: Operation::Rename {
                to: "payment_method".into(),
            },
            evidence: Evidence {
                detector: confidence,
                applicability: 1.0,
            },
        };
        let bytes = br#"{ "source" : "card", "amount":1e400 }"#;
        let outcome = support::run(
            bytes,
            &report,
            std::slice::from_ref(&candidate),
            Mode::Repair,
        );
        if samples == 200 {
            assert_eq!(outcome.assessments[0].status, Status::Applied);
            assert_eq!(
                outcome.body.as_ref(),
                br#"{ "payment_method" : "card", "amount":1e400 }"#
            );
        } else {
            assert_eq!(
                outcome.assessments[0].status,
                Status::Rejected(Reject::BelowThreshold)
            );
            support::unchanged(&outcome, bytes);
        }
        let observed = support::run(bytes, &report, &[candidate], Mode::Observe);
        support::unchanged(&observed, bytes);
        let mut semantic = after.clone();
        if let Some(n) = semantic.node.properties.get_mut("payment_method") {
            n.units = Some("new meaning".into());
        }
        let changed = compare(Side::Request, &before, &semantic);
        assert_eq!(changed.class(), Some(DriftClass::Unknown));
        let mut with_units = before.clone();
        if let Some(n) = with_units.node.properties.get_mut("source") {
            n.units = Some("old meaning".into());
        }
        assert_eq!(
            compare(Side::Request, &with_units, &semantic).class(),
            Some(DriftClass::Semantic)
        );
    }
    Ok(())
}
