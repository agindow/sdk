//! Atomic application and generated audit evidence.
use crate::{Assessment, Candidate, Failure, Limits, Mode, Operation, Outcome, Reject, Status};
use crate::{
    edits::{Edit, stage},
    spans::index,
};
use agw_audit::contracts::audit_artifact::Patch;
use agw_diff::{DriftClass, Report, Side};
use agw_inspector::{Inspection, Parse};
use std::{borrow::Cow, collections::BTreeSet};

pub fn repair<'a>(
    side: Side,
    source: &'a [u8],
    inspection: &Inspection,
    report: &Report,
    candidates: &[Candidate],
    mode: Mode,
) -> Outcome<'a> {
    repair_with_limits(
        side,
        source,
        inspection,
        report,
        candidates,
        mode,
        Limits::default(),
    )
}

pub fn repair_with_limits<'a>(
    side: Side,
    source: &'a [u8],
    inspection: &Inspection,
    report: &Report,
    candidates: &[Candidate],
    mode: Mode,
    limits: Limits,
) -> Outcome<'a> {
    let mut outcome = Outcome {
        body: Cow::Borrowed(source),
        assessments: Vec::new(),
        unresolved: Vec::new(),
        failure: None,
        patch: None,
        mode,
    };
    if !within_limits(source, report, candidates, limits) {
        outcome.failure = Some(Failure::ResourceLimit);
        return outcome;
    }
    outcome.unresolved = report.changes.clone();
    if inspection.truncated {
        outcome.failure = Some(Failure::Incomplete);
        return outcome;
    }
    if report.skipped.is_some() || report.changes.iter().any(|c| c.side != side) {
        outcome.failure = Some(Failure::InvalidReport);
        return outcome;
    }
    let value = match &inspection.parse {
        Parse::Parsed(value) => value,
        Parse::Unparseable(_) => {
            outcome.failure = Some(Failure::Unparseable);
            return outcome;
        }
    };
    if candidates.is_empty() {
        return outcome;
    }
    let spans = match index(source, value, limits) {
        Ok(spans) => spans,
        Err(failure) => {
            outcome.failure = Some(failure);
            return outcome;
        }
    };
    let mut edits = Vec::new();
    for (i, candidate) in candidates.iter().enumerate() {
        let Some(change) = report.changes.get(candidate.finding) else {
            outcome.assessments.push(Assessment {
                finding: candidate.finding,
                confidence: candidate.evidence.confidence(),
                class: DriftClass::Unknown,
                status: Status::Rejected(Reject::InvalidFinding),
            });
            continue;
        };
        let (assessment, staged) = stage(source, &spans, change, candidate, mode, i);
        if assessment.class == DriftClass::Semantic {
            outcome.unresolved[candidate.finding].class = DriftClass::Semantic;
        }
        if edits.len().saturating_add(staged.len()) > limits.max_nodes {
            outcome.failure = Some(Failure::ResourceLimit);
            return abort(outcome);
        }
        outcome.assessments.push(assessment);
        edits.extend(staged);
    }
    if outcome
        .assessments
        .iter()
        .any(|a| matches!(a.status, Status::Rejected(_)))
    {
        return abort(outcome);
    }
    if mode == Mode::Observe {
        return outcome;
    }
    edits.sort_by_key(|e| e.range.start);
    let mut destinations = BTreeSet::new();
    for edit in &edits {
        if let Some(destination) = &edit.destination
            && !destinations.insert(destination)
        {
            outcome.assessments[edit.proposal].status = Status::Rejected(Reject::Collision);
        }
    }
    for pair in edits.windows(2) {
        if pair[0].range.end > pair[1].range.start {
            for edit in pair {
                outcome.assessments[edit.proposal].status = Status::Rejected(Reject::Overlap);
            }
        }
    }
    if outcome
        .assessments
        .iter()
        .any(|a| matches!(a.status, Status::Rejected(_)))
    {
        return abort(outcome);
    }
    if edits.is_empty() {
        return outcome;
    }
    let body = match apply(source, &edits, limits.max_bytes) {
        Some(body) => body,
        None => {
            outcome.failure = Some(Failure::ResourceLimit);
            return abort(outcome);
        }
    };
    let fields = edits
        .iter()
        .map(|e| e.field.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(TryInto::try_into)
        .collect::<Result<Vec<_>, _>>();
    let fields = match fields {
        Ok(fields) => fields,
        Err(_) => {
            outcome.failure = Some(Failure::InspectionMismatch);
            return abort(outcome);
        }
    };
    for edit in &edits {
        outcome.assessments[edit.proposal].status = Status::Applied;
    }
    outcome.unresolved = report
        .changes
        .iter()
        .enumerate()
        .filter(|(i, _)| {
            !outcome
                .assessments
                .iter()
                .any(|a| a.finding == *i && a.status == Status::Applied)
        })
        .map(|(_, c)| c.clone())
        .collect();
    outcome.patch = Some(Patch {
        applied: true,
        byte_identical: false,
        fields_changed: fields,
        selective: Some(true),
        reasoning: None,
    });
    outcome.body = Cow::Owned(body);
    outcome
}

fn within_limits(source: &[u8], report: &Report, candidates: &[Candidate], limits: Limits) -> bool {
    if source.len() > limits.max_bytes
        || report.changes.len() > limits.max_nodes
        || candidates.len() > limits.max_proposals
    {
        return false;
    }
    let text = report.changes.iter().fold(0usize, |sum, c| {
        sum.saturating_add(c.path.len())
            .saturating_add(c.to_path.as_ref().map_or(0, String::len))
    });
    let text = candidates.iter().fold(text, |sum, c| {
        sum.saturating_add(match &c.operation {
            Operation::Rename { to } => to.len(),
            Operation::Replace(bytes) => bytes.len(),
        })
    });
    text <= limits.max_bytes
}
fn abort(mut outcome: Outcome<'_>) -> Outcome<'_> {
    for assessment in &mut outcome.assessments {
        if assessment.status == Status::Eligible {
            assessment.status = Status::NotApplied;
        }
    }
    outcome
}
fn apply(source: &[u8], edits: &[Edit], max_bytes: usize) -> Option<Vec<u8>> {
    let size = edits.iter().try_fold(source.len(), |size, e| {
        size.checked_sub(e.range.len())?.checked_add(e.bytes.len())
    })?;
    if size > max_bytes {
        return None;
    }
    let mut body = Vec::with_capacity(size);
    let mut cursor = 0;
    for edit in edits {
        body.extend_from_slice(source.get(cursor..edit.range.start)?);
        body.extend_from_slice(&edit.bytes);
        cursor = edit.range.end;
    }
    body.extend_from_slice(source.get(cursor..)?);
    Some(body)
}
