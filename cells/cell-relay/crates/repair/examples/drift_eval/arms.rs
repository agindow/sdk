use super::{Error, model::Task};
use agw_diff::{Change, DriftClass, Kind, Reason, Report, Side};
use agw_repair::{Candidate, Evidence, Mode, Operation, repair};
use serde_json::Value;

/// Full-payload reconstruction preserves all caller fields. The comparator
/// applies supplied proposals without confidence/identity gates; it does not
/// read the scoring oracle or deliberately discard context.
fn naive(task: &Task) -> Result<Value, Error> {
    let mut value = task.input.clone();
    if let Some(proposal) = &task.proposal {
        let object = value.as_object_mut().ok_or("input must be object")?;
        if task.rename {
            if let Some(source) = object.remove("value") {
                object.insert(
                    proposal
                        .rename_to
                        .as_ref()
                        .ok_or("missing destination")?
                        .clone(),
                    source,
                );
            }
        } else {
            object.insert(task.target.clone(), proposal.value.clone());
        }
    }
    // Parse/serialize the complete object, as a full-payload repair would.
    Ok(serde_json::from_slice(&serde_json::to_vec(&value)?)?)
}

fn selective(class: &str, task: &Task) -> Result<(Option<Value>, Vec<String>), Error> {
    let (findings, candidates) = evidence(class, task);
    let source = serde_json::to_vec(&task.input)?;
    let result = repair(
        Side::Request,
        &source,
        &agw_inspector::inspect(&source, false, None),
        &findings,
        &candidates,
        Mode::Repair,
    );
    // This is the existing explicit repair/policy seam, not the full guard's
    // verifier. A discharged finding is NOT an oracle proof of correct intent.
    let verified = result.failure.is_none() && result.unresolved.is_empty();
    let decision = agw_policy::decide(
        Mode::Repair,
        agw_policy::Operation::Write,
        result.class(),
        if verified {
            agw_policy::Repair::Verified
        } else {
            agw_policy::Repair::Unavailable
        },
    );
    let trace = vec![
        format!("policy={decision:?}"),
        format!("failure={:?}", result.failure),
        format!("assessments={:?}", result.assessments),
    ];
    let sent = if decision == agw_policy::Decision::Halt {
        None
    } else {
        Some(serde_json::from_slice(result.body.as_ref())?)
    };
    Ok((sent, trace))
}

pub fn run(class: &str, task: &Task, arm: &str) -> Result<(Option<Value>, Vec<String>), Error> {
    match arm {
        "no_repair" => Ok((Some(task.input.clone()), vec![])),
        "naive" => Ok((
            Some(naive(task)?),
            vec!["full-payload reconstruction; confidence not gated".into()],
        )),
        "selective" => selective(class, task),
        _ => Err("unknown arm".into()),
    }
}

/// Shared supplied findings/candidates; neither adapter sees the scoring oracle.
pub(super) fn evidence(class: &str, task: &Task) -> (Report, Vec<Candidate>) {
    let class_kind = if task.semantic {
        DriftClass::Semantic
    } else {
        DriftClass::Shape
    };
    let (kind, reason) = if task.rename {
        (
            Kind::Renamed,
            if task.confirmed {
                Reason::ConfirmedIdentity
            } else {
                Reason::AmbiguousIdentity
            },
        )
    } else {
        match class {
            "unit_flip" | "timestamp_epoch" => (Kind::UnitsChanged, Reason::UnitsChanged),
            "enum_expansion" => (Kind::EnumExpanded, Reason::EnumExpanded),
            "null_semantics" => (Kind::NullMeaningChanged, Reason::MeaningChanged),
            _ => (Kind::TypeChanged, Reason::DeclaredStructure),
        }
    };
    let findings = Report {
        changes: vec![Change {
            side: Side::Request,
            path: "value".into(),
            to_path: task.rename.then(|| task.target.clone()),
            kind,
            class: class_kind,
            reason,
        }],
        skipped: None,
    };
    let candidates: Vec<_> = task
        .proposal
        .iter()
        .map(|p| Candidate {
            finding: 0,
            operation: if task.rename {
                Operation::Rename {
                    to: p.rename_to.clone().unwrap_or_default(),
                }
            } else {
                Operation::Replace(p.value.to_string().into_bytes())
            },
            evidence: Evidence {
                detector: p.confidence,
                applicability: 1.0,
            },
        })
        .collect();
    (findings, candidates)
}
