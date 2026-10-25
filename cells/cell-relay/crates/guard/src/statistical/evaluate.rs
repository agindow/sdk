use super::{Body, Decision, Detector, Detectors, Operation, Phase, Plan, Record};
use crate::{
    availability::{Budget, EngineFault},
    config::CallContext,
    malformed,
};
use agw_diff::{Side, Skip, compare_inspection};
use agw_inspector::{Parse, ParseClass};
use agw_policy::{Repair, decide_with_protection};
use agw_stats::{
    control,
    router::{self, CallBudget, Outbox},
    windows::{self, Availability, Limits},
};

fn limits() -> Limits {
    Limits {
        samples: 4096,
        scopes: 7,
        bytes: 2 * 1024 * 1024,
        batch: 7,
        inspection_nodes: 4096,
        inspection_bytes: 1024 * 1024,
        minimum: 1,
    }
}
fn window_fault(error: windows::Error) -> EngineFault {
    match error {
        windows::Error::ResourceLimit => EngineFault::ResourceExhausted,
        windows::Error::ProviderUnavailable | windows::Error::UnknownScope => {
            EngineFault::RegistryUnavailable
        }
        _ => EngineFault::RegistryCorrupt,
    }
}
// Charge a conservative allowance before parsing or materializing any evidence.
// Costs cover tree/key traversal, adapter copies, all fixed/enum test supports,
// routing metadata and report retention; the budget is cooperative, not time.
fn precharge(
    plan: &Plan,
    body: Body<'_>,
    side: Side,
    work: &mut Budget,
) -> Result<(), EngineFault> {
    let mut cost = body
        .bytes
        .len()
        .checked_mul(256)
        .and_then(|v| v.checked_add(16 * 1024 * 1024))
        .ok_or(EngineFault::ResourceExhausted)?;
    for field in plan.fields.iter().filter(|f| f.scope.side == side) {
        if let Availability::Ready(s) = &field.snapshot {
            let samples = s.baseline().samples().len() + s.live().samples().len() + 1;
            let local = s
                .accounted_bytes()
                .checked_mul(64)
                .and_then(|v| v.checked_add(samples * 535 * 32))
                .ok_or(EngineFault::ResourceExhausted)?;
            cost = cost
                .checked_add(local)
                .ok_or(EngineFault::ResourceExhausted)?;
        }
    }
    if let Some(selection) = plan.invariants(side) {
        cost = cost
            .checked_add(selection.allowance())
            .ok_or(EngineFault::ResourceExhausted)?;
    }
    if let Some(selection) = &plan.cross_endpoint {
        cost = cost
            .checked_add(selection.allowance())
            .ok_or(EngineFault::ResourceExhausted)?;
    }
    work.charge(cost)
}
#[allow(clippy::too_many_arguments)]
pub(super) fn phase<D: Detectors>(
    plan: &Plan,
    audit_ref: &str,
    workflow: Option<&crate::cross_endpoint::Workflow>,
    context: CallContext,
    operation: Operation,
    body: Body<'_>,
    side: Side,
    detectors: &D,
    work: &mut Budget,
    outbox: Option<&mut Outbox>,
    inference: &mut CallBudget,
) -> Result<Phase, EngineFault> {
    let mut phase = Phase::clean();
    if body.truncated || body.bytes.len() > context.inspection_cap() {
        phase.report.skipped = Some(Skip::Incomplete);
        return Ok(phase);
    }
    precharge(plan, body, side, work)?;
    let inspection = detectors.inspect(body.bytes);
    if inspection.truncated {
        phase.report.skipped = Some(Skip::Incomplete);
        return Ok(phase);
    }
    if let Parse::Unparseable(error) = &inspection.parse {
        if matches!(error.class, ParseClass::Data | ParseClass::Io) {
            return Err(EngineFault::Internal);
        }
        let baseline = agw_diff::Snapshot {
            node: agw_diff::Node::object([]),
            evidence: agw_diff::Evidence::Authoritative,
        };
        phase.report = compare_inspection(side, &baseline, &inspection);
        return Ok(malformed_phase(phase, context, operation));
    }
    windows::validate_inspection(&inspection, limits()).map_err(window_fault)?;
    let shape = if side == Side::Request {
        &plan.request_shape
    } else {
        &plan.response_shape
    };
    if let Some(shape) = shape {
        phase.report = compare_inspection(side, shape, &inspection);
        if phase.report.skipped.is_some() {
            return Err(EngineFault::RegistryCorrupt);
        }
        if phase
            .report
            .changes
            .iter()
            .any(|c| c.kind == agw_diff::Kind::MalformedBody)
        {
            return Ok(malformed_phase(phase, context, operation));
        }
    }
    if plan.overrides.len() > 256 {
        return Err(EngineFault::RegistryCorrupt);
    }
    if let Some(report) = body.findings {
        let validated = router::route(report, &[]).map_err(|_| EngineFault::ResourceExhausted)?;
        if validated.unavailable().is_some() {
            return Err(EngineFault::RegistryUnavailable);
        }
        if report.changes.iter().any(|c| c.side != side) {
            return Err(EngineFault::RegistryCorrupt);
        }
        phase.report.changes.extend(report.changes.iter().cloned());
    }
    let overrides: Vec<_> = plan
        .overrides
        .iter()
        .map(|o| control::Override {
            tenant: &plan.identity.tenant,
            field: &o.field,
            signal: o.signal,
            floor: o.floor,
        })
        .collect();
    let floors = control::Floors::new(plan.alpha, plan.materiality, plan.materiality, &overrides)
        .map_err(|_| EngineFault::RegistryCorrupt)?;
    for field in plan.fields.iter().filter(|f| f.scope.side == side) {
        let snapshot = match &field.snapshot {
            Availability::Ready(s) => s,
            Availability::Warming { .. } => return Err(EngineFault::NotReady),
            Availability::Unavailable(e) => return Err(window_fault(*e)),
        };
        let live = snapshot
            .with_current(&inspection, body.at, limits())
            .map_err(window_fault)?;
        for detector in [
            Detector::Unit,
            Detector::Enumeration,
            Detector::Completeness,
            Detector::TypeConsistency,
            Detector::Charset,
        ] {
            if detector == Detector::Unit
                && (snapshot.baseline().unit().is_none() || live.unit().is_none())
            {
                continue;
            }
            if detector == Detector::Enumeration && !field.enumeration {
                continue;
            }
            let evidence = detectors.run(
                detector,
                &field.scope.field,
                snapshot.baseline(),
                &live,
                plan.family(),
            )?;
            let eligibility = evidence
                .tests()
                .iter()
                .map(|test| {
                    if test.is_finding() {
                        floors
                            .evaluate(&plan.identity.tenant, &field.scope.field, test.evidence())
                            .map(Some)
                            .map_err(|_| EngineFault::Internal)
                    } else {
                        Ok(None)
                    }
                })
                .collect::<Result<Vec<_>, _>>()?;
            let record = Record {
                side,
                path: field.scope.field.clone(),
                generation: field.scope.generation.clone(),
                detector,
                evidence,
                eligibility,
            };
            phase.report.changes.extend(record.changes());
            phase.records.push(record);
        }
    }
    let enum_start = phase.report.changes.len();
    if let Some(selection) = plan.invariants(side) {
        phase.invariants = crate::invariant::evaluate(selection, &inspection)?;
        for record in phase
            .invariants
            .iter()
            .filter(|r| r.verdict == agw_invariants::Verdict::EnumAddition)
        {
            phase.report.changes.push(agw_diff::Change {
                side,
                path: record.id.clone(),
                to_path: None,
                kind: agw_diff::Kind::EnumExpanded,
                class: agw_diff::DriftClass::Semantic,
                reason: agw_diff::Reason::EnumExpanded,
            });
        }
    }
    let enum_end = phase.report.changes.len();
    if let Some(selection) = &plan.cross_endpoint {
        crate::cross_endpoint::evaluate(
            &mut phase,
            plan,
            selection,
            workflow,
            &inspection,
            body,
            side,
        )?;
    }
    phase.unavailable_invariants = phase
        .invariants
        .iter()
        .filter(|r| r.verdict == agw_invariants::Verdict::Unavailable)
        .count();
    let invariant_findings: Vec<_> = phase
        .invariants
        .iter()
        .filter(|r| r.verdict == agw_invariants::Verdict::Violation)
        .map(|r| router::Invariant {
            side,
            path: &r.id,
            rule: &r.id,
        })
        .collect();
    let routing = router::route(&phase.report, &invariant_findings)
        .map_err(|_| EngineFault::ResourceExhausted)?;
    // Only TI-20's dispatched findings reach TI-24. Aggregate decisions with
    // Halt dominant so shape/unknown findings cannot discard semantic policy.
    if routing.unavailable().is_some() {
        return Err(EngineFault::Internal);
    }
    phase.decision = routing
        .findings()
        .iter()
        .map(|dispatch| {
            let class = match dispatch.finding() {
                router::Finding::Diff(change) => change.class,
                router::Finding::Invariant(_) => agw_diff::DriftClass::Semantic,
            };
            let staged = match dispatch.finding() {
                router::Finding::Invariant(finding) => plan.invariants(side).filter(|s| {
                    s.snapshot
                        .program
                        .rules()
                        .iter()
                        .any(|r| r.id() == finding.rule)
                }),
                router::Finding::Diff(change)
                    if phase.report.changes[enum_start..enum_end]
                        .iter()
                        .any(|own| std::ptr::eq(own, change)) =>
                {
                    plan.invariants(side)
                }
                _ => None,
            };
            let mode = staged.map_or_else(
                || {
                    if let router::Finding::Invariant(f) = dispatch.finding() {
                        plan.cross_endpoint
                            .as_ref()
                            .and_then(|s| s.mode_for(f.rule, context.mode(), operation))
                            .unwrap_or(context.mode())
                    } else {
                        context.mode()
                    }
                },
                |s| s.mode(context.mode(), operation),
            );
            decide_with_protection(
                mode,
                operation,
                Some(class),
                Repair::Unavailable,
                context.fail_closed(),
            )
        })
        .max()
        .unwrap_or(Decision::Pass);
    if let Some(outbox) = outbox {
        phase.investigation = if plan.investigation_ceiling == 0 {
            if routing
                .findings()
                .iter()
                .any(|d| d.label() == router::Label::Unknown)
            {
                router::Admission::Disabled
            } else {
                router::Admission::NotNeeded
            }
        } else {
            outbox
                .submit(&routing, inference, plan.investigation_ceiling)
                .unwrap_or(router::Admission::Disabled)
        };
    } else if routing
        .findings()
        .iter()
        .any(|d| d.label() == router::Label::Unknown)
    {
        phase.investigation = router::Admission::Disabled;
    }
    if plan.invariants(side).is_some() || !phase.cross_endpoint.is_empty() {
        crate::invariant::stamp(&mut phase, plan, context, operation, body, side, audit_ref)?;
    }
    Ok(phase)
}
fn malformed_phase(mut phase: Phase, context: CallContext, operation: Operation) -> Phase {
    let disposition = malformed::disposition(context.mode(), operation);
    phase.decision = match disposition {
        malformed::Disposition::Halt => Decision::Halt,
        malformed::Disposition::Degraded => Decision::Degraded,
        _ => Decision::Pass,
    };
    phase.malformed = Some(disposition);
    phase
}
