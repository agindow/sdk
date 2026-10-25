//! Decision/detail and drift-change projection for composed audit evidence.
//!
//! The aggregate class is a maximum and changes are projected from actual
//! phase evidence only; reuse of a caller draft can never supply either.
use crate::statistical::{Outcome, Phase};
use agw_audit::contracts::audit_artifact::Change;
use agw_diff::{DriftClass as DiffClass, Kind};
use agw_policy::Decision;
use std::collections::BTreeSet;

pub(super) fn detail<O>(outcome: &Outcome<O>, malformed_write: bool) -> &'static str {
    if outcome.bypasses > 0 {
        "availability_bypass"
    } else if dominated(outcome) == Decision::Halt {
        "drift_halted"
    } else if malformed_write && outcome.mode != crate::config::Mode::Observe {
        "malformed_body_bypass"
    } else {
        match dominated(outcome) {
            Decision::Halt => "drift_halted",
            Decision::Degraded => "drift_passthrough",
            Decision::Repaired => "repair_applied",
            Decision::Pass
                if outcome.request.report.changes.is_empty()
                    && outcome
                        .response
                        .as_ref()
                        .is_none_or(|phase| phase.report.changes.is_empty()) =>
            {
                "clean"
            }
            Decision::Pass => "drift_observed",
        }
    }
}

/// The maximum TI-24 phase decision; the response may only escalate.
pub(super) fn dominated<O>(outcome: &Outcome<O>) -> Decision {
    outcome
        .response
        .as_ref()
        .map_or(outcome.request.decision, |phase| {
            outcome.request.decision.max(phase.decision)
        })
}

fn class_rank(class: DiffClass) -> u8 {
    match class {
        DiffClass::Shape => 0,
        DiffClass::Unknown => 1,
        DiffClass::Semantic => 2,
    }
}

/// Aggregate class is a maximum, so later composition can never downgrade it.
pub(super) fn combine_class(current: Option<DiffClass>, next: DiffClass) -> DiffClass {
    match current {
        Some(current) if class_rank(current) >= class_rank(next) => current,
        _ => next,
    }
}

pub(super) fn phase_class(phase: &Phase) -> Option<DiffClass> {
    let structural = phase.report.class();
    let invariant = phase
        .invariants
        .iter()
        .any(|record| record.class().is_some())
        .then_some(DiffClass::Semantic);
    match (structural, invariant) {
        (Some(a), Some(b)) => Some(combine_class(Some(a), b)),
        (value, None) | (None, value) => value,
    }
}

fn kind_name(kind: Kind) -> &'static str {
    match kind {
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
    }
}

pub(super) fn report_change(change: &agw_diff::Change) -> Option<Change> {
    Some(Change {
        path: change.path.clone().try_into().ok()?,
        kind: kind_name(change.kind).try_into().ok()?,
        from: None,
        to: change.to_path.clone(),
    })
}

pub(super) fn invariant_changes(phase: &Phase) -> Vec<Change> {
    let cross: BTreeSet<&str> = phase
        .cross_endpoint
        .iter()
        .filter(|comparison| {
            comparison.reason == agw_invariants::cross_endpoint::Reason::Divergence
        })
        .map(|comparison| comparison.id.as_str())
        .collect();
    let mut changes = Vec::new();
    for record in &phase.invariants {
        if record.class().is_none() || cross.contains(record.id.as_str()) {
            continue;
        }
        let kind = if record.verdict == agw_invariants::Verdict::EnumAddition {
            "enum_expanded"
        } else {
            "invariant_violated"
        };
        let Ok(path) = record.id.clone().try_into() else {
            continue;
        };
        let Ok(change_kind) = kind.try_into() else {
            continue;
        };
        changes.push(Change {
            path,
            kind: change_kind,
            from: record.actual.clone(),
            to: record.expected.clone(),
        });
    }
    for comparison in &phase.cross_endpoint {
        if comparison.reason != agw_invariants::cross_endpoint::Reason::Divergence {
            continue;
        }
        let from = comparison
            .left
            .as_ref()
            .and_then(|observation| observation.value)
            .map(|value| value.to_string());
        let to = comparison
            .right
            .as_ref()
            .and_then(|observation| observation.value)
            .map(|value| value.to_string());
        let Ok(path) = comparison.id.clone().try_into() else {
            continue;
        };
        let Ok(change_kind) = "cross_endpoint_diverged".try_into() else {
            continue;
        };
        changes.push(Change {
            path,
            kind: change_kind,
            from,
            to,
        });
    }
    changes
}

pub(super) fn change_key(change: &Change) -> (String, String, String, String) {
    (
        change.path.to_string(),
        change.kind.to_string(),
        change
            .from
            .as_ref()
            .map_or_else(String::new, |value| value.to_string()),
        change
            .to
            .as_ref()
            .map_or_else(String::new, |value| value.to_string()),
    )
}
