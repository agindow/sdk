use agw_invariants::activation::Generation;
use serde_json::json;

/// Shared serialization keeps local and paired qualification evidence identical.
pub(crate) fn metadata(generation: &Generation) -> serde_json::Value {
    let d = generation.decision();
    json!({
        "generation": generation.number(), "stage": format!("{:?}", generation.stage()),
        "policy": d.policy, "candidate": d.candidate, "reason": format!("{:?}", d.reason),
        "authorization": d.authorization.as_ref().map(|p| json!({
            "expires": p.expires, "version_min": p.versions.0, "version_max": p.versions.1,
            "modes": p.modes.iter().map(|m| format!("{m:?}")).collect::<Vec<_>>(),
            "coverage_ppm": p.thresholds.coverage_ppm, "minimum_valid": p.thresholds.minimum_valid,
            "minimum_invalid": p.thresholds.minimum_invalid, "false_positive_ppm": p.thresholds.false_positive_ppm,
            "missed_violation_ppm": p.thresholds.missed_violation_ppm,
            "calibration": p.calibration.as_ref().map(|cal| json!({ "dataset": cal.dataset,
                "candidate": cal.candidate, "sampling": cal.sampling, "independent_production": cal.independent_production,
                "eligible": cal.counts.eligible, "evaluated": cal.counts.evaluated, "valid": cal.counts.valid,
                "invalid": cal.counts.invalid, "false_positives": cal.counts.false_positives, "missed_violations": cal.counts.missed_violations })),
        })),
        "receipts": d.receipts.iter().map(|r| json!({"id": r.id, "training": r.training, "holdout": r.holdout,
            "policy": r.policy, "stage": format!("{:?}",r.stage), "independent": r.independent, "complete": r.complete,
            "started": r.started, "finished": r.finished, "expires": r.expires, "eligible": r.counts.eligible, "evaluated": r.counts.evaluated,
            "valid": r.counts.valid, "invalid": r.counts.invalid, "false_positives": r.counts.false_positives,
            "missed_violations": r.counts.missed_violations})).collect::<Vec<_>>(),
    })
}
