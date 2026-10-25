use super::{Body, Phase, Pipeline};
use crate::{
    availability::{Budget, EngineFault},
    config::{CallContext, Mode},
};
use agw_diff::{Change, DriftClass, Kind, Reason, Report, Side};
use agw_inspector::{Parse, ParseClass};
use agw_policy::{Decision, Operation, Repair, decide_with_protection};
use std::borrow::Cow;

pub(super) fn evaluate<'a>(
    context: CallContext,
    operation: Operation,
    body: Body<'a>,
    side: Side,
    pipeline: &impl Pipeline,
    work: &mut Budget,
) -> Result<Phase<'a>, EngineFault> {
    if body.truncated || body.bytes.len() > context.inspection_cap() {
        return Err(EngineFault::ResourceExhausted);
    }
    work.charge(body.bytes.len().saturating_mul(16).saturating_add(4096))?;
    let inspection = agw_inspector::inspect(body.bytes, false, None);
    let mut malformed = false;
    let evidence = if let Parse::Unparseable(error) = &inspection.parse {
        if matches!(error.class, ParseClass::Data | ParseClass::Io) {
            return Err(EngineFault::Internal);
        }
        malformed = true;
        super::Evidence {
            report: Report {
                changes: vec![Change {
                    side,
                    path: "$".into(),
                    to_path: None,
                    kind: Kind::MalformedBody,
                    class: DriftClass::Shape,
                    reason: Reason::CompleteMalformedBody,
                }],
                skipped: None,
            },
            candidates: Vec::new(),
        }
    } else {
        agw_stats::windows::validate_inspection(
            &inspection,
            agw_stats::windows::Limits {
                samples: 1,
                scopes: 1,
                bytes: context.inspection_cap(),
                batch: 1,
                inspection_nodes: 4096,
                inspection_bytes: context.inspection_cap(),
                minimum: 1,
            },
        )
        .map_err(|_| EngineFault::ResourceExhausted)?;
        pipeline.detect(side, body.bytes, work)?
    };
    let report = evidence.report;
    if report.skipped.is_some()
        || report
            .changes
            .iter()
            .any(|c| c.side != side || c.path.is_empty())
    {
        return Err(EngineFault::RegistryCorrupt);
    }
    if report.changes.len() > 4096 {
        return Err(EngineFault::ResourceExhausted);
    }
    // Also covers a complete valid JSON body with a contract-invalid root shape.
    malformed |= report.changes.iter().any(|c| c.kind == Kind::MalformedBody);
    if malformed
        && (report.changes.len() != 1
            || !evidence.candidates.is_empty()
            || report.class() != Some(DriftClass::Shape))
    {
        return Err(EngineFault::RegistryCorrupt);
    }
    let mut phase = Phase {
        body: Cow::Borrowed(body.bytes),
        report,
        decision: Decision::Pass,
        fault: None,
        reason: "clean",
        patch: None,
        confidence: None,
        malformed,
        proposals: evidence.candidates,
        assessments: Vec::new(),
        verification: crate::review::Verification::NotRun,
        changes: Vec::new(),
    };
    if malformed {
        phase.reason = "malformed_body";
        phase.confidence = Some(0.0);
        phase.decision = if context.mode() == Mode::Observe {
            Decision::Pass
        } else {
            decide_with_protection(
                context.mode(),
                operation,
                Some(DriftClass::Shape),
                Repair::Unavailable,
                context.fail_closed(),
            )
        };
    } else {
        let repaired = agw_repair::repair(
            side,
            body.bytes,
            &inspection,
            &phase.report,
            &phase.proposals,
            context.mode(),
        );
        if repaired.failure.is_some() {
            return Err(EngineFault::Internal);
        }
        let applied: Vec<_> = repaired
            .assessments
            .iter()
            .filter(|a| a.status == agw_repair::Status::Applied)
            .map(|a| a.finding)
            .collect();
        let verified =
            !applied.is_empty() && pipeline.verify(side, &repaired.body, &applied, work)?;
        phase.verification = if applied.is_empty() {
            crate::review::Verification::NotRun
        } else if verified {
            crate::review::Verification::Passed
        } else {
            crate::review::Verification::Failed
        };
        let unresolved = if verified {
            repaired.class()
        } else {
            // Numeric coercion refusals can upgrade the supplied classification.
            let mut unresolved = phase.report.clone();
            for assessment in &repaired.assessments {
                if assessment.class == DriftClass::Semantic
                    && let Some(change) = unresolved.changes.get_mut(assessment.finding)
                {
                    change.class = DriftClass::Semantic;
                }
            }
            phase.report = unresolved;
            phase.report.class()
        };
        phase.decision = if let Some(class) = unresolved {
            decide_with_protection(
                context.mode(),
                operation,
                Some(class),
                Repair::Unavailable,
                context.fail_closed(),
            )
        } else if verified {
            Decision::Repaired
        } else {
            Decision::Pass
        };
        phase.reason = if context.mode() == Mode::Observe && !phase.report.changes.is_empty() {
            "observe_only"
        } else if !applied.is_empty() && !verified {
            "repair_verification_failed"
        } else if unresolved.is_some() {
            "unresolved_drift"
        } else if verified {
            "verified_repair"
        } else {
            "clean"
        };
        if verified && phase.decision != Decision::Halt {
            phase.patch = repaired.patch().cloned();
            phase.confidence = repaired.confidence();
            phase.body = repaired.body;
        }
        phase.assessments = repaired.assessments;
    }
    // Validate artifact fields within availability containment, before dispatch.
    let text = phase.report.changes.iter().fold(0usize, |sum, c| {
        sum.saturating_add(c.path.len())
            .saturating_add(c.to_path.as_ref().map_or(0, String::len))
    });
    work.charge(text)?;
    if text > 1024 * 1024 {
        return Err(EngineFault::ResourceExhausted);
    }
    phase.changes = phase
        .report
        .changes
        .iter()
        .map(super::artifact::change)
        .collect::<Result<_, _>>()?;
    Ok(phase)
}
