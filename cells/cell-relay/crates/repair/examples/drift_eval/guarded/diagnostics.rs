//! Explain native refusals without altering fixture labels or outcome scoring.
use super::super::{Error, Label, Outcome, arms, fixtures, model::Fixture};
use super::{Record, Report};
use agw_diff::Side;
use agw_repair::{Mode, Reject, Status};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Serialize)]
pub struct CaseDiagnosis {
    pub id: String,
    pub label: Label,
    pub cause: String,
}
#[derive(Debug, Serialize)]
pub struct Diagnosis {
    pub total: usize,
    pub repairable_total: usize,
    pub without_unique_evidence: usize,
    pub repairable_numeric_refusals: usize,
    pub counts: BTreeMap<String, usize>,
    pub cases: Vec<CaseDiagnosis>,
}
fn cause(fixture: &Fixture, record: &Record) -> Result<String, Error> {
    if record.case.recovered {
        return Ok("recovered".into());
    }
    if record.bypasses != 0 {
        return Ok("availability_bypass".into());
    }
    if record.case.outcome == Outcome::UnsafeWrite {
        return Ok("unsafe_dispatch".into());
    }
    if record.verification.iter().any(|v| v == "refused") {
        return Ok("independent_verification_refused".into());
    }
    let (findings, candidates) = arms::evidence(&fixture.class, &fixture.task);
    if candidates.is_empty() {
        return Ok("missing_proposal".into());
    }
    // Repeat the pure patch assessment offline to obtain typed refusal reasons.
    // These diagnostics never supply a plan, change the guard or rescore a case.
    let source = serde_json::to_vec(&fixture.task.input)?;
    let result = agw_repair::repair(
        Side::Request,
        &source,
        &agw_inspector::inspect(&source, false, None),
        &findings,
        &candidates,
        Mode::Repair,
    );
    if result.failure.is_some() {
        return Ok("capture_or_resource_failure".into());
    }
    let reason = result.assessments.iter().find_map(|a| match a.status {
        Status::Rejected(reason) => Some(reason),
        _ => None,
    });
    Ok(match reason {
        Some(Reject::NumericCoercion) => "numeric_coercion_fence".into(),
        Some(Reject::BelowThreshold) => "confidence_below_threshold".into(),
        Some(Reject::UnconfirmedIdentity) => "unconfirmed_identity".into(),
        Some(Reject::Unsupported) => "unsupported_operation".into(),
        Some(other) => format!("native_refusal:{other:?}"),
        None => "unresolved_without_verified_repair".into(),
    })
}
pub fn diagnose(report: &Report) -> Result<Diagnosis, Error> {
    let fixtures = fixtures()?;
    if report.healthy.len() != fixtures.len() {
        return Err("incomplete diagnosis input".into());
    }
    let mut cases = Vec::new();
    let mut counts = BTreeMap::new();
    let mut repairable_total = 0;
    let mut repairable_numeric_refusals = 0;
    for (fixture, record) in fixtures.iter().zip(&report.healthy) {
        if record.case.id != fixture.id || record.case.label != fixture.oracle.label {
            return Err("diagnosis case identity/label mismatch".into());
        }
        let cause = cause(fixture, record)?;
        *counts.entry(cause.clone()).or_insert(0) += 1;
        if record.case.label == Label::Repairable {
            repairable_total += 1;
            repairable_numeric_refusals += usize::from(cause == "numeric_coercion_fence");
        }
        cases.push(CaseDiagnosis {
            id: fixture.id.clone(),
            label: record.case.label,
            cause,
        });
    }
    Ok(Diagnosis {
        total: cases.len(),
        repairable_total,
        without_unique_evidence: cases.len() - repairable_total,
        repairable_numeric_refusals,
        counts,
        cases,
    })
}
