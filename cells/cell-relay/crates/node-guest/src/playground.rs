//! Explicit reviewed synthetic fixture only, never installed in Guest::begin.
//! Trusted migration identity is committed independently of caller/schema data.
use agw_diff::{Change, DriftClass, Kind, Reason, Report, Side};
use agw_guard::{
    availability::{Budget, EngineFault},
    config::{Config, RuntimeConfig},
    gated::{Body, Call, Evidence, Pipeline},
    readiness::AvailabilityGate,
};
use agw_repair::{Candidate, Operation};
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize)]
struct Reviewed {
    migration: String,
    identity: String,
    source: String,
    target: String,
    before: String,
    after: String,
}
impl Pipeline for Reviewed {
    fn detect(&self, side: Side, bytes: &[u8], work: &mut Budget) -> Result<Evidence, EngineFault> {
        work.charge(bytes.len())?;
        // Exact fixture admission excludes arbitrary uploads and ambiguous mappings.
        if side != Side::Request || bytes != self.before.as_bytes() {
            return Ok(Evidence::default());
        }
        Ok(Evidence {
            report: Report {
                changes: vec![Change {
                    side,
                    path: self.source.clone(),
                    to_path: Some(self.target.clone()),
                    kind: Kind::Renamed,
                    class: DriftClass::Shape,
                    reason: Reason::ConfirmedIdentity,
                }],
                skipped: None,
            },
            candidates: vec![Candidate {
                finding: 0,
                operation: Operation::Rename {
                    to: self.target.clone(),
                },
                evidence: agw_repair::Evidence {
                    detector: 1.0,
                    applicability: 1.0,
                },
            }],
        })
    }
    fn verify(
        &self,
        side: Side,
        patched: &[u8],
        findings: &[usize],
        work: &mut Budget,
    ) -> Result<bool, EngineFault> {
        work.charge(patched.len())?;
        // Independent reviewed target bytes check intended meaning AND all untouched
        // bytes. Schema acceptance/confidence alone cannot establish rename intent.
        Ok(side == Side::Request && findings == [0] && patched == self.after.as_bytes())
    }
}

pub fn run(bytes: &[u8], mode: &str, work: u32) -> Result<String, String> {
    let reviewed: Reviewed = serde_json::from_str(include_str!(
        "../../../../../contracts/src/docs/fixtures/playground-rename.json"
    ))
    .map_err(|e| e.to_string())?;
    let config = RuntimeConfig::from_sources(
        Config {
            mode: Some(mode.into()),
            ..Config::default()
        },
        None,
    )
    .map_err(|e| e.to_string())?;
    let mut draft: agw_audit::contracts::audit_artifact::AuditArtifact = serde_json::from_str(
        include_str!("../../../../../contracts/examples/audit_artifact.valid.json"),
    )
    .map_err(|e| e.to_string())?;
    draft.operation = agw_policy::Operation::Write;
    let gate = AvailabilityGate::new(true);
    let outcome = Call::new(
        config.begin_call(None).map_err(|e| e.to_string())?,
        &gate,
        work as usize,
    )
    .execute(Body::complete(bytes), &reviewed, draft, |_| {
        ((), Body::complete(b"{}"))
    });
    let phase = &outcome.request;
    let applied = phase.patch.as_ref().is_some_and(|patch| patch.applied);
    Ok(json!({
        "migration": reviewed.migration, "identity": reviewed.identity,
        "before": std::str::from_utf8(bytes).map_err(|e| e.to_string())?,
        "after": std::str::from_utf8(&phase.body).map_err(|e| e.to_string())?,
        "repairApplied": applied, "verified": phase.reason == "verified_repair",
        "halt": phase.decision == agw_policy::Decision::Halt,
        "bypassed": phase.fault.is_some(), "reason": phase.reason,
        "patch": phase.patch, "findings": phase.report.changes.len(),
        "upstreamCalled": false,
    })
    .to_string())
}
