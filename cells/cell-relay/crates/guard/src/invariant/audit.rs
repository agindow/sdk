use super::Record;
use crate::{
    availability::EngineFault,
    config::CallContext,
    statistical::{Body, Decision, Operation, Phase, Plan},
};
use agw_audit::contracts::audit_artifact::{
    AuditArtifact, AuditArtifactDecision, Change, Drift, DriftClass, DriftSide, Patch, Proof,
    ProofVerifiedAgainst,
};
use agw_diff::Side;
use agw_invariants::Verdict;
use serde_json::json;

#[allow(clippy::too_many_arguments)]
pub(crate) fn stamp(
    phase: &mut Phase,
    plan: &Plan,
    context: CallContext,
    operation: Operation,
    body: Body<'_>,
    side: Side,
    audit_ref: &str,
) -> Result<(), EngineFault> {
    let fault = |_| EngineFault::Internal;
    let findings: Vec<_> = phase
        .invariants
        .iter()
        .filter(|r| r.class().is_some())
        .collect();
    let decision = match phase.decision {
        Decision::Halt => AuditArtifactDecision::Blocked,
        Decision::Degraded => AuditArtifactDecision::Degraded,
        Decision::Repaired => AuditArtifactDecision::Repaired,
        Decision::Pass if phase.unavailable_invariants > 0 && findings.is_empty() => {
            AuditArtifactDecision::Bypassed
        }
        _ => AuditArtifactDecision::Ok,
    };
    let changes = findings
        .iter()
        .map(|r| {
            Ok(Change {
                path: r.id.clone().try_into().map_err(fault)?,
                kind: if phase.cross_endpoint.iter().any(|c| {
                    c.id == r.id && c.reason == agw_invariants::cross_endpoint::Reason::Divergence
                }) {
                    "cross_endpoint_diverged"
                } else if r.verdict == Verdict::EnumAddition {
                    "enum_expanded"
                } else {
                    "invariant_violated"
                }
                .try_into()
                .map_err(fault)?,
                from: r.actual.clone(),
                to: r.expected.clone(),
            })
        })
        .collect::<Result<Vec<_>, EngineFault>>()?;
    let drift = if findings.is_empty() {
        None
    } else {
        Some(Drift {
            class: DriftClass::Semantic,
            side: if side == Side::Request {
                DriftSide::Request
            } else {
                DriftSide::Response
            },
            vendor: Some(plan.identity.vendor.clone().try_into().map_err(fault)?),
            changes,
            first_seen: None,
            occurrences: None,
        })
    };
    let snapshot = plan.invariants(side).map(|s| &s.snapshot);
    let contract_version = snapshot
        .map(|s| s.scope.contract_version)
        .or_else(|| plan.cross_endpoint.as_ref().map(|s| s.contract_version))
        .ok_or(EngineFault::Internal)?;
    let mut metadata = serde_json::Map::new();
    metadata.insert("agindow_invariants".into(), json!({
        "tenant": plan.identity.tenant, "connection": plan.identity.connection,
        "version": snapshot.map(|s| s.version), "contract_version": contract_version,
        "pin": snapshot.and_then(|s| s.controls.pin), "disabled": snapshot.map(|s| &s.controls.disabled),
        "overrides": snapshot.map(|s| s.controls.overrides.iter().map(|r| r.id.as_str()).collect::<Vec<_>>()),
        "unavailable": phase.unavailable_invariants,
        "records": phase.invariants.iter().map(record).collect::<Vec<_>>(),
        "scope": "per-phase invariant draft; no write-integrity proof",
    }));
    let activation = plan
        .invariants(side)
        .and_then(|s| s.activation.as_deref())
        .or_else(|| {
            plan.cross_endpoint
                .as_ref()
                .filter(|_| !phase.cross_endpoint.is_empty())
                .and_then(|s| s.activation.as_deref())
        });
    if let Some(generation) = activation {
        metadata.insert(
            "agindow_activation".into(),
            super::activation::metadata(generation),
        );
    }
    if let Some(selection) = &plan.cross_endpoint {
        metadata.insert(
            "agindow_cross_endpoint".into(),
            crate::cross_endpoint::metadata(
                selection,
                phase.cross_scope.as_ref(),
                &phase.cross_endpoint,
            ),
        );
    }
    let audit = AuditArtifact {
        id: format!(
            "{audit_ref}.{}",
            if side == Side::Request {
                "request"
            } else {
                "response"
            }
        )
        .try_into()
        .map_err(fault)?,
        ts: body.at,
        tool: plan.identity.operation.clone().try_into().map_err(fault)?,
        mode: context.mode(),
        operation,
        decision,
        drift,
        patch: Some(Patch {
            applied: false,
            byte_identical: true,
            fields_changed: Vec::new(),
            selective: None,
            reasoning: None,
        }),
        proof: Proof {
            contract_version: contract_version.to_string().try_into().map_err(fault)?,
            verified_against: if snapshot.is_some_and(|s| s.controls.pin.is_some()) {
                ProofVerifiedAgainst::Pin
            } else {
                ProofVerifiedAgainst::Cache
            },
            write_integrity: false,
            idempotency_key: None,
        },
        confidence: None,
        hypothesis: None,
        detail: Some("invariant_evaluation".try_into().map_err(fault)?),
        note: None,
        metadata,
    };
    if !findings.is_empty() {
        phase.warning =
            Some("semantic invariant evidence: original bytes retained; no verified repair");
        phase.headers.claim("X-Agindow-Drift", "detected");
        phase.headers.claim("X-Agindow-Drift-Class", "semantic");
        phase
            .headers
            .claim("X-Agindow-Status", audit.decision.to_string());
        phase.headers.claim("X-Agindow-Repair", "skipped");
        phase
            .headers
            .claim("X-Agindow-Mode", context.mode().to_string());
    } else if phase.unavailable_invariants > 0 {
        phase.warning = Some("invariant evidence unavailable; no write-integrity proof");
        phase
            .headers
            .claim("X-Agindow-Status", audit.decision.to_string());
    }
    phase.invariant_audit = Some(audit);
    Ok(())
}
fn record(r: &Record) -> serde_json::Value {
    json!({ "id": r.id, "verdict": format!("{:?}", r.verdict), "actual": r.actual, "expected": r.expected,
        "severity": if r.verdict == Verdict::EnumAddition { "info" } else if r.verdict == Verdict::Violation { "error" } else { "none" } })
}
