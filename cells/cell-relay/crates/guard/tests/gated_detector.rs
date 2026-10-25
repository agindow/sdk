#[path = "support/gated.rs"]
mod support;
use agw_diff::{Evidence as Source, Node, Side, Snapshot};
use agw_guard::{
    availability::{Budget, EngineFault},
    config::{Config, RuntimeConfig},
    gated::{Body, Call, Evidence, Pipeline},
    readiness::AvailabilityGate,
};
use agw_inspector::inference::JsonType;
use agw_repair::{Candidate, Operation};

struct SchemaChange {
    before: Snapshot,
    after: Snapshot,
}
impl Pipeline for SchemaChange {
    fn detect(&self, side: Side, _: &[u8], work: &mut Budget) -> Result<Evidence, EngineFault> {
        work.charge(4096)?;
        let report = agw_diff::compare(side, &self.before, &self.after);
        Ok(Evidence {
            report,
            candidates: vec![Candidate {
                finding: 0,
                operation: Operation::Rename { to: "new".into() },
                evidence: agw_repair::Evidence {
                    detector: 1.0,
                    applicability: 1.0,
                },
            }],
        })
    }
    fn verify(
        &self,
        _: Side,
        bytes: &[u8],
        findings: &[usize],
        work: &mut Budget,
    ) -> Result<bool, EngineFault> {
        work.charge(4096)?;
        assert_eq!(findings, [0]);
        let inspection = agw_inspector::inspect(bytes, false, None);
        // Independent target schema validation, not the proposal's assertion.
        let report = agw_diff::compare_inspection(Side::Request, &self.after, &inspection);
        Ok(report.skipped.is_none() && report.changes.is_empty())
    }
}
#[test]
fn real_shape_detector_and_target_verification_feed_selective_execution()
-> Result<(), Box<dyn std::error::Error>> {
    let mut scalar = Node::scalar(JsonType::Number);
    scalar.identity = Some("stable-field".into());
    let before = Snapshot {
        node: Node::object([("old".into(), scalar.clone())]),
        evidence: Source::Authoritative,
    };
    let mut node = Node::object([("new".into(), scalar)]);
    node.required.insert("new".into());
    // Requiredness is the same on both sides so the only change is identity-backed rename.
    let mut before = before;
    before.node.required.insert("old".into());
    let after = Snapshot {
        node,
        evidence: Source::Authoritative,
    };
    let pipeline = SchemaChange { before, after };
    let runtime = RuntimeConfig::from_sources(Config::default(), None)?;
    let gate = AvailabilityGate::new(true);
    let source = b"{ \"old\": 1e400 }";
    let result = Call::new(runtime.begin_call(None)?, &gate, 100_000).execute(
        Body::complete(source),
        &pipeline,
        support::draft()?,
        |sent| {
            assert_eq!(sent, b"{ \"new\": 1e400 }");
            ("upstream", Body::complete(source))
        },
    );
    assert_eq!(result.audit.decision.to_string(), "repaired");
    assert_eq!(
        result.response.ok_or("response")?.body.as_ref(),
        b"{ \"new\": 1e400 }"
    );
    // Every composed artifact remains serializable through the frozen generated boundary.
    let encoded = serde_json::to_vec(&result.audit)?;
    let decoded: agw_audit::contracts::audit_artifact::AuditArtifact =
        serde_json::from_slice(&encoded)?;
    assert_eq!(serde_json::to_vec(&decoded)?, encoded);
    Ok(())
}
