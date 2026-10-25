//! Scope, readiness and suppression provenance for composed evidence (TI-47).
//! No credentials, payload values or persistent-history claims are emitted.
use crate::statistical::{Outcome, Phase, Plan, Record};
use agw_diff::Side;
use agw_stats::windows::Availability;
use serde_json::{Value, json};

fn side_name(side: Side) -> &'static str {
    match side {
        Side::Request => "request",
        Side::Response => "response",
    }
}

/// The selected identity and per-field generation/readiness. Warming or
/// unavailable snapshots are recorded as such; they never become clean evidence.
pub(super) fn fields(plan: Option<&Plan>) -> Vec<Value> {
    let Some(plan) = plan else {
        return Vec::new();
    };
    plan.fields
        .iter()
        .map(|field| {
            let scope = &field.scope;
            let mut value = json!({
                "side": side_name(scope.side),
                "path": scope.field,
                "window": scope.window,
                "generation": scope.generation,
                "segment": scope.segment,
            });
            match &field.snapshot {
                Availability::Ready(snapshot) => {
                    value["readiness"] = json!("ready");
                    value["baseline_samples"] = json!(snapshot.baseline().samples().len());
                    value["live_samples"] = json!(snapshot.live().samples().len());
                }
                Availability::Warming { baseline, live } => {
                    value["readiness"] = json!("warming");
                    value["baseline_samples"] = json!(baseline);
                    value["live_samples"] = json!(live);
                }
                Availability::Unavailable(error) => {
                    value["readiness"] = json!("unavailable");
                    value["error"] = json!(format!("{error:?}"));
                }
            }
            value
        })
        .collect()
}

/// `None` without an active plan: the default engine does no statistical work.
pub(super) fn identity(plan: Option<&Plan>) -> Value {
    match plan {
        Some(plan) => json!({
            "tenant": plan.identity.tenant,
            "connection": plan.identity.connection,
            "vendor": plan.identity.vendor,
            "operation": plan.identity.operation,
            "segment": plan.identity.segment,
        }),
        None => Value::Null,
    }
}

pub(super) fn summary(records: &[Record]) -> Value {
    let mut tests = 0usize;
    let mut admitted = 0usize;
    let mut non_findings = 0usize;
    let mut not_significant = 0usize;
    let mut below_materiality = 0usize;
    let mut insufficient = 0usize;
    for record in records {
        tests += record.evidence.tests().len();
        if !record.evidence.sufficient() {
            insufficient += 1;
        }
        for (index, test) in record.evidence.tests().iter().enumerate() {
            match record.eligibility.get(index) {
                Some(Some(decision)) if decision.eligible() => admitted += 1,
                Some(Some(agw_stats::control::Decision::NotSignificant)) => not_significant += 1,
                Some(Some(agw_stats::control::Decision::BelowMateriality)) => {
                    below_materiality += 1;
                }
                _ if !test.is_finding() => non_findings += 1,
                _ => {}
            }
        }
    }
    json!({
        "tests": tests,
        "admitted": admitted,
        "non_finding": non_findings,
        "not_significant": not_significant,
        "below_materiality": below_materiality,
        "insufficient_records": insufficient,
    })
}

fn invariant_counts(phase: &Phase) -> Value {
    let mut pass = 0usize;
    let mut violation = 0usize;
    let mut enum_addition = 0usize;
    let mut unavailable = 0usize;
    for record in &phase.invariants {
        match record.verdict {
            agw_invariants::Verdict::Pass => pass += 1,
            agw_invariants::Verdict::Violation => violation += 1,
            agw_invariants::Verdict::EnumAddition => enum_addition += 1,
            agw_invariants::Verdict::Unavailable => unavailable += 1,
        }
    }
    json!({
        "pass": pass,
        "violation": violation,
        "enum_addition": enum_addition,
        "unavailable": unavailable,
    })
}

/// Per-phase readiness/suppression provenance: faults, skips, malformed bodies,
/// materiality outcomes and invariant availability all stay visible.
pub(super) fn evaluation(phase: Option<&Phase>) -> Value {
    let Some(phase) = phase else {
        return json!({"evaluated": false});
    };
    json!({
        "evaluated": true,
        "skipped": phase.report.skipped.map(|skip| format!("{skip:?}")),
        "fault": phase.fault.map(|fault| format!("{fault:?}")),
        "malformed": phase.malformed.map(|disposition| format!("{disposition:?}")),
        "investigation": format!("{:?}", phase.investigation),
        "warning": phase.warning,
        "invariants": invariant_counts(phase),
        "cross_endpoint": {
            "comparisons": phase.cross_endpoint.len(),
            "divergences": phase.cross_endpoint.iter().filter(|c|
                c.reason == agw_invariants::cross_endpoint::Reason::Divergence).count(),
        },
        "summary": summary(&phase.records),
    })
}

/// The final TI-24 disposition and per-phase policy inputs, nothing new.
pub(super) fn policy(outcome: &Outcome<impl Sized>) -> Value {
    json!({
        "request": {
            "fault": outcome.request.fault.map(|fault| format!("{fault:?}")),
            "decision": format!("{:?}", outcome.request.decision),
        },
        "response": outcome.response.as_ref().map(|phase| json!({
            "fault": phase.fault.map(|fault| format!("{fault:?}")),
            "decision": format!("{:?}", phase.decision),
        })),
        "dispatched": outcome.dispatched,
        "bypasses": outcome.bypasses,
        "fail_closed": outcome.context.fail_closed(),
        "investigation_reserved": outcome.reserved,
    })
}

/// Existing per-side invariant/cross provenance, reused verbatim under one key.
pub(super) fn invariant_provenance(
    phase: Option<&Phase>,
) -> Option<serde_json::Map<String, Value>> {
    let artifact = phase?.invariant_audit.as_ref()?;
    let mut subtree = serde_json::Map::new();
    for (source, target) in [
        ("agindow_invariants", "selection"),
        ("agindow_cross_endpoint", "cross_endpoint"),
        ("agindow_activation", "activation"),
    ] {
        if let Some(value) = artifact.metadata.get(source) {
            subtree.insert(target.into(), value.clone());
        }
    }
    (!subtree.is_empty()).then_some(subtree)
}
