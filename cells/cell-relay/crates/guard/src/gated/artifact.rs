use super::Phase;
use crate::{
    availability::EngineFault,
    config::{CallContext, Mode},
    proxy::Headers,
};
use agw_audit::contracts::audit_artifact::{
    AuditArtifact, AuditArtifactDecision as Verdict, Change, Drift, DriftSide, Patch,
};
use agw_diff::{Kind, Side};
use agw_policy::{Decision, Operation};

pub(super) fn change(c: &agw_diff::Change) -> Result<Change, EngineFault> {
    let kind = match c.kind {
        Kind::Renamed => "field_renamed",
        Kind::Removed => "field_removed",
        Kind::Added => "field_added",
        Kind::OptionalAdded => "optional_field_added",
        Kind::RequiredAdded => "required_field_added",
        Kind::RequiredChanged => "required_changed",
        Kind::TypeChanged => "type_changed",
        Kind::UnitsChanged => "unit_changed",
        Kind::EnumExpanded => "enum_expanded",
        Kind::EnumChanged => "enum_changed",
        Kind::NullMeaningChanged => "null_meaning_changed",
        Kind::MalformedBody => "malformed_body",
    };
    Ok(Change {
        path: c
            .path
            .clone()
            .try_into()
            .map_err(|_| EngineFault::Internal)?,
        kind: kind.try_into().map_err(|_| EngineFault::Internal)?,
        from: None,
        to: c.to_path.clone(),
    })
}
pub(super) fn compose(
    context: CallContext,
    mut draft: AuditArtifact,
    request: &Phase<'_>,
    response: Option<&Phase<'_>>,
    bypasses: u8,
) -> (AuditArtifact, Headers, Option<&'static str>) {
    let phases: Vec<_> = std::iter::once((Side::Request, request))
        .chain(response.map(|p| (Side::Response, p)))
        .collect();
    let dominant = phases
        .iter()
        .map(|(_, p)| *p)
        .max_by_key(|p| (p.decision, !p.report.changes.is_empty()))
        .unwrap_or(request);
    let malformed_write = phases.iter().any(|(_, p)| p.malformed)
        && draft.operation != Operation::Read
        && context.mode() != Mode::Strict;
    draft.decision = if bypasses > 0 {
        Verdict::Bypassed
    } else if dominant.decision == Decision::Halt {
        Verdict::Blocked
    } else if malformed_write && context.mode() != Mode::Observe {
        Verdict::Bypassed
    } else {
        match dominant.decision {
            Decision::Pass => Verdict::Ok,
            Decision::Repaired => Verdict::Repaired,
            Decision::Degraded => Verdict::Degraded,
            Decision::Halt => Verdict::Blocked,
        }
    };
    draft.mode = context.mode();
    draft.detail = if bypasses > 0 {
        "availability_bypass"
    } else {
        dominant.reason
    }
    .try_into()
    .ok();
    draft.hypothesis = None;
    draft.note = None;
    draft.proof.write_integrity = false;
    // Payload verification is not idempotency/write verification.
    draft.proof.idempotency_key = None;
    draft.confidence = phases
        .iter()
        .filter_map(|(_, p)| p.confidence)
        .reduce(f64::min);
    let class = phases
        .iter()
        .filter_map(|(_, p)| p.report.class())
        .max_by_key(|c| match c {
            agw_diff::DriftClass::Shape => 0,
            agw_diff::DriftClass::Unknown => 1,
            agw_diff::DriftClass::Semantic => 2,
        });
    let req = !request.report.changes.is_empty();
    let resp = response.is_some_and(|p| !p.report.changes.is_empty());
    draft.drift = class.map(|class| Drift {
        class: match class {
            agw_diff::DriftClass::Shape => agw_audit::contracts::audit_artifact::DriftClass::Shape,
            agw_diff::DriftClass::Semantic => {
                agw_audit::contracts::audit_artifact::DriftClass::Semantic
            }
            agw_diff::DriftClass::Unknown => {
                agw_audit::contracts::audit_artifact::DriftClass::Unknown
            }
        },
        side: if req && resp {
            DriftSide::Both
        } else if req {
            DriftSide::Request
        } else {
            DriftSide::Response
        },
        changes: phases.iter().flat_map(|(_, p)| p.changes.clone()).collect(),
        vendor: None,
        first_seen: None,
        occurrences: None,
    });
    let fields: Vec<_> = phases
        .iter()
        .filter_map(|(side, p)| p.patch.as_ref().map(|patch| (side, patch)))
        .flat_map(|(side, patch)| {
            patch.fields_changed.iter().filter_map(move |field| {
                let side = if *side == Side::Request {
                    "request"
                } else {
                    "response"
                };
                format!("{side}:{}", field.as_str()).try_into().ok()
            })
        })
        .collect();
    draft.patch = if fields.is_empty() {
        phases.iter().any(|(_, p)| p.malformed).then_some(Patch {
            applied: false,
            byte_identical: true,
            fields_changed: Vec::new(),
            selective: None,
            reasoning: None,
        })
    } else {
        Some(Patch {
            applied: true,
            byte_identical: false,
            fields_changed: fields,
            selective: Some(true),
            reasoning: None,
        })
    };
    let mut headers = Headers::new();
    headers.claim("X-Agindow-Mode", draft.mode.to_string());
    headers.claim("X-Agindow-Status", draft.decision.to_string());
    headers.claim("X-Agindow-Audit", draft.id.to_string());
    headers.claim(
        "X-Agindow-Drift",
        if draft.drift.is_some() {
            "detected"
        } else {
            "none"
        },
    );
    if let Some(drift) = &draft.drift {
        headers.claim("X-Agindow-Drift-Class", drift.class.to_string());
        headers.claim("X-Agindow-Drift-Side", drift.side.to_string());
    }
    headers.claim(
        "X-Agindow-Repair",
        if draft.patch.as_ref().is_some_and(|patch| patch.applied) {
            "applied"
        } else if phases
            .iter()
            .any(|(_, p)| p.reason == "repair_verification_failed")
        {
            "failed"
        } else {
            "skipped"
        },
    );
    if let Some(patch) = &draft.patch
        && patch.applied
    {
        headers.claim(
            "X-Agindow-Patch",
            format!("selective:{}", patch.fields_changed.len()),
        );
    }
    if let Some(confidence) = draft.confidence {
        headers.claim("X-Agindow-Confidence", format!("{confidence:.2}"));
    }
    draft.metadata.insert("agindow_policy".into(), serde_json::json!({
        "request": { "reason": request.reason, "fault": request.fault.map(|f| format!("{f:?}")),
            "decision": format!("{:?}", request.decision) },
        "response": response.map(|p| serde_json::json!({"reason": p.reason,
            "fault": p.fault.map(|f| format!("{f:?}")), "decision": format!("{:?}", p.decision)})),
        "fail_closed": context.fail_closed(),
        "dispatched": request.decision != Decision::Halt,
        "bypasses": bypasses,
    }));
    let warning = if bypasses > 0 {
        Some("availability_bypass: original outcome retained")
    } else if draft.decision == Verdict::Blocked {
        Some("drift_halted: human escalation required")
    } else if draft.decision == Verdict::Degraded || malformed_write {
        Some("drift_passthrough: original bytes retained")
    } else {
        None
    };
    (draft, headers, warning)
}
