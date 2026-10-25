//! Bounded string-enum novelty evidence (TI-16, runtime contract §10.20).
//!
//! Callers select enum candidates, segmented baselines and complete windows.
//! Timestamp minima describe only this window; no global first-ever history,
//! clock reads, payload rewriting, policy decisions or I/O are performed.
mod audit;
use std::collections::{BTreeMap, BTreeSet};

use agw_diff::DriftClass;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    InvalidInput,
    ResourceLimit,
    NoFinding,
    AuditConversion,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}

/// Exact decoded categorical string with a caller-supplied observation time.
/// Empty strings are categories; null/absence are not string observations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Observation<'a> {
    pub value: &'a str,
    pub at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cardinalities {
    pub baseline: usize,
    pub observed: usize,
    pub union: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Evidence {
    InsufficientEvidence,
    Unchanged,
    BelowMateriality,
    Semantic,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewValue {
    pub value: String,
    pub count: usize,
    pub frequency: f64,
    pub first_seen: DateTime<Utc>,
    pub material: bool,
}

/// Owns bounded evidence only. Private construction prevents callers from
/// promoting below-floor novelty into an audit finding by editing flags.
#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    path: String,
    cardinalities: Cardinalities,
    evidence: Evidence,
    new_values: Vec<NewValue>,
}
impl Report {
    pub fn path(&self) -> &str {
        &self.path
    }
    pub fn cardinalities(&self) -> Cardinalities {
        self.cardinalities
    }
    pub fn evidence(&self) -> Evidence {
        self.evidence
    }
    pub fn new_values(&self) -> &[NewValue] {
        &self.new_values
    }
    pub fn class(&self) -> Option<DriftClass> {
        (self.evidence == Evidence::Semantic).then_some(DriftClass::Semantic)
    }
}

fn validate(path: &str, baseline: &[&str], observations: &[Observation<'_>]) -> Result<(), Error> {
    if path.is_empty() {
        return Err(Error::InvalidInput);
    }
    if baseline.len() > 256 || observations.len() > 4096 {
        return Err(Error::ResourceLimit);
    }
    let mut text = 0;
    for value in std::iter::once(path)
        .chain(baseline.iter().copied())
        .chain(observations.iter().map(|o| o.value))
    {
        if value.len() > 4096 {
            return Err(Error::ResourceLimit);
        }
        text += value.len();
        if text > 1024 * 1024 {
            return Err(Error::ResourceLimit);
        }
    }
    Ok(())
}

/// Compare a known string vocabulary with timestamped observations. Frequency
/// uses all observations as its denominator, including known categories.
/// New categories never require a mean shift, and frequency shifts within the
/// baseline vocabulary alone never count as enum drift.
pub fn compare(
    path: &str,
    baseline: &[&str],
    observations: &[Observation<'_>],
) -> Result<Report, Error> {
    validate(path, baseline, observations)?;
    let known: BTreeSet<_> = baseline.iter().copied().collect();
    if known.len() != baseline.len() {
        return Err(Error::InvalidInput);
    }
    let mut seen = BTreeMap::<&str, (usize, DateTime<Utc>)>::new();
    for observation in observations {
        let entry = seen.entry(observation.value).or_insert((0, observation.at));
        entry.0 += 1;
        entry.1 = entry.1.min(observation.at);
        if seen.len() > 256 {
            return Err(Error::ResourceLimit);
        }
    }
    let cardinalities = Cardinalities {
        baseline: known.len(),
        observed: seen.len(),
        union: known.len() + seen.keys().filter(|value| !known.contains(**value)).count(),
    };
    let enough = !known.is_empty() && observations.len() >= 20;
    let new_values: Vec<_> = seen
        .iter()
        .filter(|(value, _)| !known.is_empty() && !known.contains(**value))
        .map(|(value, (count, first_seen))| {
            let frequency = *count as f64 / observations.len() as f64;
            NewValue {
                value: (*value).into(),
                count: *count,
                frequency,
                first_seen: *first_seen,
                material: enough && *count >= 2 && frequency >= 0.01,
            }
        })
        .collect();
    let evidence = if !enough {
        Evidence::InsufficientEvidence
    } else if new_values.iter().any(|v| v.material) {
        Evidence::Semantic
    } else if new_values.is_empty() {
        Evidence::Unchanged
    } else {
        Evidence::BelowMateriality
    };
    Ok(Report {
        path: path.into(),
        cardinalities,
        evidence,
        new_values,
    })
}
