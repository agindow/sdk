//! Offline silent replay of labeled detector evidence through TI-19 controls.
//! This is not raw-traffic collection or detector execution. Callers must obtain
//! evidence from actual seasonal comparisons and review labels/provenance.
use super::{Error, Evidence, Floors, Season, Window, select, text};

#[derive(Debug, Clone, Copy)]
pub enum Provenance<'a> {
    Synthetic,
    /// Caller attestation only; reviewers must independently verify the corpus.
    Production {
        dataset_id: &'a str,
    },
}
#[derive(Debug, Clone, Copy)]
pub struct Opportunity<'a> {
    pub current: Window<'a>,
    pub history: &'a [Window<'a>],
    pub evidence: Evidence,
    pub actual_drift: bool,
}
#[derive(Debug, Clone, Copy)]
pub struct Day<'a> {
    pub utc_day: u64,
    /// Actual traffic, independent of how many hypotheses were tested.
    pub traffic: u64,
    pub opportunities: &'a [Opportunity<'a>],
}
#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    pub negatives: usize,
    pub false_positives: usize,
    pub positives: usize,
    pub true_positives: usize,
    pub unavailable: usize,
    pub false_positive_rate: f64,
    pub within_threshold: bool,
    /// Requires reviewed production provenance outside this library too.
    pub production_qualified: bool,
}
/// Run 30 consecutive populated UTC days. No alert budget is accepted here:
/// notification throttling must never improve detector quality measurements.
/// At most 100,000 opportunities and 10,000,000 history-slot visits per run.
pub fn run(
    days: &[Day<'_>],
    season: Season,
    floors: &Floors<'_>,
    threshold: f64,
    provenance: Provenance<'_>,
) -> Result<Report, Error> {
    if days.len() != 30 || !threshold.is_finite() || threshold <= 0.0 || threshold >= 1.0 {
        return Err(Error::InvalidInput);
    }
    let production = match provenance {
        Provenance::Synthetic => false,
        Provenance::Production { dataset_id } => {
            text(dataset_id)?;
            true
        }
    };
    // Bound all aggregate work before invoking selection or materiality.
    let mut count = 0usize;
    let mut visits = 0usize;
    for (index, day) in days.iter().enumerate() {
        count = count
            .checked_add(day.opportunities.len())
            .ok_or(Error::ResourceLimit)?;
        if count > 100_000 {
            return Err(Error::ResourceLimit);
        }
        if day.traffic == 0
            || !day.opportunities.iter().any(|item| !item.actual_drift)
            || (index > 0 && days[index - 1].utc_day.checked_add(1) != Some(day.utc_day))
        {
            return Err(Error::InvalidInput);
        }
        for item in day.opportunities {
            if item.current.start / 86_400 != day.utc_day {
                return Err(Error::InvalidInput);
            }
            let work = item
                .history
                .len()
                .checked_mul(season.periods)
                .ok_or(Error::ResourceLimit)?;
            visits = visits.checked_add(work).ok_or(Error::ResourceLimit)?;
            if visits > 10_000_000 {
                return Err(Error::ResourceLimit);
            }
        }
    }
    if days[29].traffic <= days[0].traffic {
        return Err(Error::InvalidInput);
    }
    let mut report = Report {
        negatives: 0,
        false_positives: 0,
        positives: 0,
        true_positives: 0,
        unavailable: 0,
        false_positive_rate: 0.0,
        within_threshold: false,
        production_qualified: false,
    };
    for day in days {
        for item in day.opportunities {
            let decision = floors.evaluate(
                item.current.scope.tenant,
                item.current.scope.field,
                item.evidence,
            )?;
            let available = select(&item.current, item.history, season)?.is_some();
            if !available {
                report.unavailable += 1;
            }
            if item.actual_drift {
                report.positives += 1;
                if available && decision.eligible() {
                    report.true_positives += 1;
                }
            } else {
                report.negatives += 1;
                if available && decision.eligible() {
                    report.false_positives += 1;
                }
            }
        }
    }
    report.false_positive_rate = report.false_positives as f64 / report.negatives as f64;
    report.within_threshold = report.false_positive_rate < threshold;
    report.production_qualified = production && report.within_threshold && report.unavailable == 0;
    Ok(report)
}
