use super::model::Metrics;
use super::{ARMS, CLASSES, Error, Label, Report, fixtures, scoring};

fn same_rate(a: Option<f64>, b: Option<f64>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => (a - b).abs() <= 1e-15,
        (None, None) => true,
        _ => false,
    }
}
pub(super) fn same_metrics(actual: &Metrics, expected: &Metrics) -> bool {
    // Counts and all regression decisions are exact. JSON float parsing can
    // differ by one ULP, so tolerate that only for derived display rates.
    if !same_rate(Some(actual.recovery_rate), Some(expected.recovery_rate))
        || !same_rate(
            actual.repairable_recovery_rate,
            expected.repairable_recovery_rate,
        )
        || !same_rate(Some(actual.violation_rate), Some(expected.violation_rate))
        || !same_rate(
            actual.violations_per_dispatch,
            expected.violations_per_dispatch,
        )
    {
        return false;
    }
    let mut normalized = actual.clone();
    normalized.recovery_rate = expected.recovery_rate;
    normalized.repairable_recovery_rate = expected.repairable_recovery_rate;
    normalized.violation_rate = expected.violation_rate;
    normalized.violations_per_dispatch = expected.violations_per_dispatch;
    normalized == *expected
}
fn validate(report: &Report) -> Result<(), Error> {
    if report.version != 2 || report.cases.len() != 126 {
        return Err("incompatible version or incomplete cases".into());
    }
    let recomputed = scoring::metrics(&report.cases);
    if report.metrics.len() != recomputed.len()
        || !report
            .metrics
            .iter()
            .zip(&recomputed)
            .all(|(a, b)| same_metrics(a, b))
    {
        return Err("missing or inconsistent mandatory metrics".into());
    }
    for (fixture, cases) in fixtures()?.iter().zip(report.cases.as_chunks::<3>().0) {
        for (case, arm) in cases.iter().zip(ARMS) {
            if case.id != fixture.id
                || case.class != fixture.class
                || case.arm != arm
                || case.label != fixture.oracle.label
            {
                return Err("case identity/order/label changed".into());
            }
            let rescored = scoring::score(fixture, arm, case.sent.clone(), case.trace.clone());
            if *case != rescored {
                return Err(
                    "case metrics disagree with sent payload and independent oracle".into(),
                );
            }
        }
    }
    Ok(())
}
pub fn compare(current: &Report, previous: &Report) -> Result<(), Error> {
    if current.corpus != previous.corpus {
        return Err("incompatible corpus; explicit version migration required".into());
    }
    validate(current)?;
    validate(previous)?;
    for (now, old) in current.cases.iter().zip(&previous.cases) {
        if (old.recovered && !now.recovered) || (!old.violation && now.violation) {
            return Err(format!(
                "regression: {} / {} ({:?} -> {:?})",
                now.id, now.arm, old.outcome, now.outcome
            )
            .into());
        }
    }
    // Safety must not be bought by discarding recovery. The per-case check
    // above independently fails an always-blocking replacement on repairable cases.
    for class in CLASSES {
        for label in [
            "all",
            Label::Repairable.name(),
            Label::Ambiguous.name(),
            Label::Unsupported.name(),
        ] {
            let violations = |arm: &str| {
                current
                    .cases
                    .iter()
                    .filter(|c| {
                        c.arm == arm
                            && c.class == class
                            && (label == "all" || c.label.name() == label)
                            && c.violation
                    })
                    .count()
            };
            if violations("selective") > violations("no_repair") {
                return Err(format!("selective safety exceeds no-repair: {class}/{label}").into());
            }
        }
    }
    Ok(())
}
