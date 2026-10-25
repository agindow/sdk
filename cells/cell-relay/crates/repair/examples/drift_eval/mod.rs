//! Versioned internal conditional repair-seam eval; no detector-quality claim.
mod arms;
#[allow(dead_code)]
pub mod guarded;
mod model;
mod regression;
mod scoring;
use model::Fixture;
pub use model::{Label, Outcome, Report};
pub use regression::compare;
pub use scoring::score;
use std::collections::BTreeSet;
pub const CLASSES: [&str; 7] = [
    "unit_flip",
    "timestamp_epoch",
    "enum_expansion",
    "null_semantics",
    "type_coercion",
    "date_format",
    "field_rename",
];
pub const ARMS: [&str; 3] = ["no_repair", "naive", "selective"];
const CORPUS: &str = include_str!("cases.json");
type Error = Box<dyn std::error::Error>;

pub fn fixtures() -> Result<Vec<Fixture>, Error> {
    let fixtures: Vec<Fixture> = serde_json::from_str(CORPUS)?;
    let ids: BTreeSet<_> = fixtures.iter().map(|f| &f.id).collect();
    if fixtures.len() != 42 || ids.len() != 42 {
        return Err("expected 42 unique cases".into());
    }
    for class in CLASSES {
        for label in [Label::Repairable, Label::Ambiguous, Label::Unsupported] {
            if fixtures
                .iter()
                .filter(|f| f.class == class && f.oracle.label == label)
                .count()
                != 2
            {
                return Err(format!("expected two {label:?} cases for {class}").into());
            }
        }
    }
    for f in &fixtures {
        if f.oracle.rationale.is_empty()
            || f.task.input == f.oracle.expected
            || !scoring::accepted(f, &f.oracle.expected)
        {
            return Err(format!("invalid oracle: {}", f.id).into());
        }
        if (f.oracle.label == Label::Unsupported) != f.task.proposal.is_none() {
            return Err(format!("proposal availability disagrees with label: {}", f.id).into());
        }
        if let Some(p) = &f.task.proposal
            && (!p.confidence.is_finite()
                || !(0.0..=1.0).contains(&p.confidence)
                || (f.task.rename && p.rename_to.is_none()))
        {
            return Err(format!("invalid proposal: {}", f.id).into());
        }
    }
    Ok(fixtures)
}
impl Report {
    pub fn refresh_metrics(&mut self) {
        self.metrics = scoring::metrics(&self.cases);
    }
}
pub fn run() -> Result<Report, Error> {
    let mut cases = Vec::new();
    for f in fixtures()? {
        for arm in ARMS {
            let (sent, trace) = arms::run(&f.class, &f.task, arm)?;
            cases.push(score(&f, arm, sent, trace));
        }
    }
    let mut report = Report {
        version: 2,
        corpus: CORPUS.into(),
        cases,
        metrics: Vec::new(),
    };
    report.refresh_metrics();
    Ok(report)
}
pub fn summary(report: &Report) -> String {
    let mut lines = vec!["INTERNAL v2 conditional repair-seam eval; synthetic, not detector/production calibration".into(),
        "class / label / arm: safe recovery, repairable recovery, successful repairs, blocks, upstream rejections, unsafe writes (per all / per dispatch)".into()];
    for m in &report.metrics {
        let eligible = m.repairable_recovery_rate.map_or_else(
            || "N/A".into(),
            |r| format!("{}/{} ({r:.3})", m.repairable_recovered, m.repairable_total),
        );
        let dispatched = m
            .violations_per_dispatch
            .map_or_else(|| "N/A".into(), |r| format!("{r:.3}"));
        lines.push(format!(
            "{} / {} / {}: {}/{} ({:.3}), {}, {}, {}, {}, {} ({:.3} / {})",
            m.class,
            m.label,
            m.arm,
            m.safe_writes,
            m.total,
            m.recovery_rate,
            eligible,
            m.successful_repairs,
            m.blocked,
            m.rejected,
            m.unsafe_writes,
            m.violation_rate,
            dispatched
        ));
    }
    lines.join("\n")
}
