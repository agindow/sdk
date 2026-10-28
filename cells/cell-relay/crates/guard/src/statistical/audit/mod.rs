//! TI-47: one composed audit artifact from all statistical guard evidence.
//!
//! TI-24 remains the decision owner and TI-29 owns storage/retrieval. This
//! boundary owns deterministic, bounded composition only: no detector runs, no
//! policy changes, no repair, no persistence and no schema change. The frozen
//! audit v1 artifact carries the result; the reserved `agindow_statistical`
//! metadata subtree carries typed per-finding evidence (contract §10.41).
mod bounds;
mod changes;
mod evidence;
mod provenance;
use crate::statistical::{Outcome, Plan};
use agw_audit::contracts::audit_artifact::{
    AuditArtifact, AuditArtifactDecision as Verdict, AuditArtifactOperation as Operation, Change,
    Drift, DriftClass, DriftSide, Patch,
};
use agw_audit::publish::{Publication, Publisher};
use agw_diff::{DriftClass as DiffClass, Side};
use agw_policy::Decision;
use changes::{
    change_key, combine_class, detail, dominated, invariant_changes, phase_class, report_change,
};
use chrono::{DateTime, Utc};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub use bounds::{MAX_CHANGES, MAX_FINDINGS, METADATA_KEY};

/// One caller-verified patch that was actually applied to the outgoing or
/// incoming payload. Constructing this asserts independent verification;
/// composition never verifies patches itself and never proves write integrity.
#[derive(Debug, Clone, Copy)]
pub struct Applied<'a> {
    pub side: Side,
    pub fields_changed: &'a [String],
    /// True only after the host independently verified the applied patch.
    pub verified: bool,
    pub selective: bool,
}

/// Keys this boundary owns on the final artifact. Reused drafts cannot leak
/// per-phase statistical, invariant, activation or enum drafts through them.
const RESERVED_METADATA: [&str; 6] = [
    METADATA_KEY,
    "agindow_policy",
    "agindow_invariants",
    "agindow_cross_endpoint",
    "agindow_activation",
    "agindow_enum_growth",
];

impl<O> Outcome<O> {
    /// Deterministically compose this call's statistical evidence into the
    /// supplied draft and return the one final per-call artifact.
    #[must_use = "Publish the returned artifact; composition has no effect otherwise"]
    pub fn compose(&self, draft: AuditArtifact) -> AuditArtifact {
        compose_with(self, draft, &[])
    }

    /// Compose with caller-verified applied patches. Unverified entries are
    /// ignored rather than reported as applied.
    #[must_use = "Publish the returned artifact; composition has no effect otherwise"]
    pub fn compose_with(&self, draft: AuditArtifact, applied: &[Applied<'_>]) -> AuditArtifact {
        compose_with(self, draft, applied)
    }

    /// Non-blocking local publication through TI-29's seam. A store or honesty
    /// refusal is returned as `None`; a full outbox remains a successful
    /// publication with `queued: false`. The call outcome and composed artifact
    /// are unaffected and nothing is awaited.
    pub fn publish(
        &self,
        draft: AuditArtifact,
        publisher: &mut Publisher,
        now: DateTime<Utc>,
    ) -> Option<Publication> {
        publisher.publish(self.compose(draft), now).ok()
    }
}

/// Compose without caller-verified repairs.
pub fn compose<O>(outcome: &Outcome<O>, draft: AuditArtifact) -> AuditArtifact {
    compose_with(outcome, draft, &[])
}

/// The single composition function. Every admitted finding is retained in a
/// deterministic order; composition order cannot erase evidence or downgrade
/// the aggregate class.
pub fn compose_with<O>(
    outcome: &Outcome<O>,
    mut draft: AuditArtifact,
    applied: &[Applied<'_>],
) -> AuditArtifact {
    let plan = outcome
        .plan
        .as_deref()
        .filter(|plan| !plan.has_no_selection());
    let mut entries = Vec::new();
    let mut changes: BTreeMap<(String, String, String, String), Change> = BTreeMap::new();
    let (mut request_drift, mut response_drift) = (false, false);
    let mut class: Option<DiffClass> = None;
    for (phase, side) in [
        (Some(&outcome.request), Side::Request),
        (outcome.response.as_ref(), Side::Response),
    ] {
        let Some(phase) = phase else {
            continue;
        };
        let mut phase_drift = false;
        for change in &phase.report.changes {
            if let Some(change) = report_change(change) {
                phase_drift = true;
                changes.insert(change_key(&change), change);
            }
        }
        for change in invariant_changes(phase) {
            phase_drift = true;
            changes.insert(change_key(&change), change);
        }
        if let Some(phase_class) = phase_class(phase) {
            class = Some(combine_class(class, phase_class));
        }
        if phase_drift {
            match side {
                Side::Request => request_drift = true,
                Side::Response => response_drift = true,
            }
        }
        for record in &phase.records {
            entries.extend(evidence::statistical(record));
        }
        entries.extend(evidence::auxiliary(phase, side));
    }
    let (findings, findings_omitted, finding_bytes) = evidence::ordered(entries);
    let changes: Vec<Change> = changes.into_values().collect();
    let changes_omitted = changes.len().saturating_sub(MAX_CHANGES);
    let changes: Vec<Change> = changes.into_iter().take(MAX_CHANGES).collect();

    let malformed_write = outcome.operation != Operation::Read
        && outcome.mode != crate::config::Mode::Strict
        && [Some(&outcome.request), outcome.response.as_ref()]
            .into_iter()
            .flatten()
            .any(|phase| phase.malformed.is_some());
    let decision = if outcome.bypasses > 0 {
        Verdict::Bypassed
    } else if dominated(outcome) == Decision::Halt {
        Verdict::Blocked
    } else if malformed_write && outcome.mode != crate::config::Mode::Observe {
        Verdict::Bypassed
    } else {
        match dominated(outcome) {
            Decision::Halt => Verdict::Blocked,
            Decision::Degraded => Verdict::Degraded,
            Decision::Repaired => Verdict::Repaired,
            Decision::Pass => Verdict::Ok,
        }
    };
    let drift = class.map(|class| Drift {
        class: match class {
            DiffClass::Shape => DriftClass::Shape,
            DiffClass::Semantic => DriftClass::Semantic,
            DiffClass::Unknown => DriftClass::Unknown,
        },
        side: if request_drift && response_drift {
            DriftSide::Both
        } else if request_drift {
            DriftSide::Request
        } else {
            DriftSide::Response
        },
        vendor: plan
            .map(|plan| plan.identity.vendor.clone().try_into())
            .transpose()
            .ok()
            .flatten(),
        changes,
        first_seen: None,
        occurrences: None,
    });
    draft.operation = outcome.operation;
    draft.mode = outcome.mode;
    draft.decision = decision;
    draft.drift = drift;
    draft.patch = applied_patch(applied);
    draft.confidence = None;
    draft.hypothesis = None;
    if let Some(kind) = sole_hypothesis(&findings) {
        draft.hypothesis = kind.try_into().ok();
    }
    draft.note = None;
    draft.detail = detail(outcome, malformed_write).try_into().ok();
    draft.proof.write_integrity = false;
    draft.proof.idempotency_key = None;
    let mut metadata = std::mem::take(&mut draft.metadata);
    for key in RESERVED_METADATA {
        metadata.remove(key);
    }
    // The statistical plan is frozen for this call and is therefore the
    // authoritative tenant identity. A reusable caller draft may have come
    // from another call; retaining its tenant would make the final artifact
    // internally contradictory and cause TI-29's tenant-bound publisher to
    // reject otherwise valid evidence.
    if let Some(plan) = plan {
        metadata.insert("tenant".into(), Value::String(plan.identity.tenant.clone()));
    }
    metadata.insert("agindow_policy".into(), provenance::policy(outcome));
    metadata.insert(
        METADATA_KEY.into(),
        composition_metadata(
            outcome,
            plan,
            findings,
            findings_omitted,
            changes_omitted,
            finding_bytes,
        ),
    );
    draft.metadata = metadata;
    draft
}

fn composition_metadata<O>(
    outcome: &Outcome<O>,
    plan: Option<&Plan>,
    findings: Vec<Value>,
    findings_omitted: usize,
    changes_omitted: usize,
    finding_bytes: bool,
) -> Value {
    let mut stats = json!({
        "schema": bounds::SCHEMA,
        "note": "statistical evidence only; no conversion, repair or write-integrity permission",
        "identity": provenance::identity(plan),
        "fields": provenance::fields(plan),
        "phases": {
            "request": provenance::evaluation(Some(&outcome.request)),
            "response": outcome.response.as_ref().map(|phase| provenance::evaluation(Some(phase))),
        },
        "policy": provenance::policy(outcome),
        "findings": findings,
        "limits": {
            "findings": MAX_FINDINGS,
            "changes": MAX_CHANGES,
            "finding_bytes": bounds::MAX_FINDING_BYTES,
        },
        "truncated": {
            "findings": findings_omitted,
            "changes": changes_omitted,
            "finding_bytes": finding_bytes,
        },
        "provenance": {
            "baseline": "frozen selected generation; window-only",
            "history": "window_first_seen is not persistent first-ever history; recurrence belongs to TI-29",
            "conversion": "unit hypotheses never authorize conversion or repair",
            "write_integrity": "not established by statistical composition",
        },
    });
    let mut invariants = Map::new();
    if let Some(subtree) = provenance::invariant_provenance(Some(&outcome.request)) {
        invariants.insert("request".into(), Value::Object(subtree));
    }
    if let Some(subtree) = provenance::invariant_provenance(outcome.response.as_ref()) {
        invariants.insert("response".into(), Value::Object(subtree));
    }
    if !invariants.is_empty() {
        stats["invariants"] = Value::Object(invariants);
    }
    stats
}

fn applied_patch(applied: &[Applied<'_>]) -> Option<Patch> {
    let verified: Vec<_> = applied
        .iter()
        .filter(|entry| entry.verified && !entry.fields_changed.is_empty())
        .collect();
    let fields: Vec<_> = verified
        .iter()
        .flat_map(|entry| {
            let side = if entry.side == Side::Request {
                "request"
            } else {
                "response"
            };
            entry
                .fields_changed
                .iter()
                .map(move |field| format!("{side}:{field}"))
        })
        .filter_map(|field| field.try_into().ok())
        .collect();
    (!fields.is_empty()).then(|| Patch {
        applied: true,
        byte_identical: false,
        fields_changed: fields,
        selective: Some(verified.iter().all(|entry| entry.selective)),
        reasoning: None,
    })
}

fn sole_hypothesis(findings: &[Value]) -> Option<String> {
    let kinds: BTreeSet<&str> = findings
        .iter()
        .flat_map(|finding| finding["hypotheses"].as_array().into_iter().flatten())
        .filter_map(|hypothesis| hypothesis["kind"].as_str())
        .collect();
    (kinds.len() == 1)
        .then(|| kinds.into_iter().next().map(str::to_string))
        .flatten()
}
