use super::{Error, Evidence, MAX_BYTES, ReviewCase, Scope};
use agw_audit::contracts::audit_artifact::AuditArtifact;
use agw_policy::Decision;
use serde_json::{Value, json};
use std::io::{self, Write};
struct Bounded(Vec<u8>);
impl Write for Bounded {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.0.len().saturating_add(bytes.len()) > MAX_BYTES {
            return Err(io::Error::other("review byte limit"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
#[derive(serde::Serialize)]
struct Draft<'a> {
    contract: &'static str,
    version: u8,
    id: &'a str,
    created_at: chrono::DateTime<chrono::Utc>,
    scope: &'a Scope,
    audit: &'a AuditArtifact,
    dispatched: bool,
    evidence: Vec<Value>,
}
fn decision(value: Decision) -> &'static str {
    match value {
        Decision::Pass => "pass",
        Decision::Repaired => "repaired",
        Decision::Degraded => "degraded",
        Decision::Halt => "halt",
    }
}
pub(super) fn build(
    audit: &AuditArtifact,
    evidence: &Evidence,
    scope: &Scope,
) -> Result<ReviewCase, Error> {
    scope.validate(audit)?;
    // Check caller metadata before recursive serialization; the final envelope
    // is checked again with phase evidence included.
    let mut nodes = 0;
    for value in audit.metadata.values() {
        check_tree(value, 3, &mut nodes)?;
    }
    let phases = evidence.phases.iter().map(|phase| {
        let proposals: Vec<_> = phase.proposals.iter().enumerate().map(|(index, candidate)| {
            let (kind, to, replacement) = match &candidate.operation {
                agw_repair::Operation::Rename { to } => ("rename", Some(to.as_str()), None),
                agw_repair::Operation::Replace(bytes) => ("replace", None, Some(bytes.iter().map(|byte| format!("{byte:02x}")).collect::<String>())),
            };
            let assessment = phase.assessments.get(index);
            json!({"finding": candidate.finding.to_string(), "kind": kind, "to": to, "replacement_hex": replacement,
                "detector": candidate.evidence.detector.to_string(), "applicability": candidate.evidence.applicability.to_string(),
                "confidence": assessment.and_then(|a| a.confidence), "assessment": assessment.map(|a| format!("{:?}", a.status)).unwrap_or_else(|| "Unavailable".into())})
        }).collect();
        let changes: Vec<_> = phase.report.changes.iter().map(|change| json!({
            "path": change.path, "to": change.to_path, "kind": format!("{:?}", change.kind), "class": change.class.to_string(), "reason": format!("{:?}", change.reason)
        })).collect();
        json!({"side": phase.side, "decision": decision(phase.decision), "reason": phase.reason,
            "confidence": phase.confidence, "verification": phase.verification, "changes": changes, "proposals": proposals})
    }).collect();
    let draft = Draft {
        contract: "review_case",
        version: 1,
        id: audit.id.as_str(),
        created_at: audit.ts,
        scope,
        audit,
        dispatched: evidence.dispatched,
        evidence: phases,
    };
    let mut bounded = Bounded(Vec::new());
    serde_json::to_writer(&mut bounded, &draft).map_err(|_| Error::ResourceLimit)?;
    let value: Value = serde_json::from_slice(&bounded.0).map_err(|_| Error::InvalidEnvelope)?;
    check_tree(&value, 0, &mut 0)?;
    serde_json::from_value(value).map_err(|_| Error::InvalidEnvelope)
}

fn check_tree(value: &Value, depth: usize, nodes: &mut usize) -> Result<(), Error> {
    *nodes += 1;
    if depth > 64 || *nodes > 50_000 {
        return Err(Error::ResourceLimit);
    }
    match value {
        Value::Array(items) => {
            for item in items {
                check_tree(item, depth + 1, nodes)?;
            }
        }
        Value::Object(items) => {
            for item in items.values() {
                check_tree(item, depth + 1, nodes)?;
            }
        }
        _ => (),
    }
    Ok(())
}
