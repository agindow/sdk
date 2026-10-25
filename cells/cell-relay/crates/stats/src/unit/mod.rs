//! TI-15 unit hypotheses: statistical diagnosis only, never a repair proposal.
//! Finite f64 projections are caller-owned; do not round high-precision tokens
//! into this API. See runtime contract §10.19 for limits and threshold formulas.
mod metrics;
use agw_audit::contracts::audit_artifact::{AuditArtifact, Change, Drift, DriftClass, DriftSide};
use agw_diff::Side;
pub use metrics::{Metrics, measure};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    InvalidInput,
    ResourceLimit,
    NumericRange,
    AuditConversion,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Conversion {
    Times100,
    Divide100,
    Times1000,
    Divide1000,
}
impl Conversion {
    pub const fn factor(self) -> f64 {
        match self {
            Self::Times100 => 100.,
            Self::Divide100 => 0.01,
            Self::Times1000 => 1000.,
            Self::Divide1000 => 0.001,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UnitConversion {
    pub baseline_median: f64,
    pub observed_median: f64,
    pub ratio: f64,
    pub conversion: Conversion,
    pub tolerance: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Detection {
    InsufficientEvidence,
    Stable(Metrics),
    Drift(Finding),
}

/// Privately constructed after both evidence gates pass. No payload or repair
/// candidate is carried: a hypothesis cannot perform a numeric conversion.
#[derive(Debug, Clone, PartialEq)]
pub struct Finding {
    path: String,
    metrics: Metrics,
    hypothesis: Option<UnitConversion>,
}
impl Finding {
    pub fn class(&self) -> agw_diff::DriftClass {
        agw_diff::DriftClass::Semantic
    }
    pub fn metrics(&self) -> Metrics {
        self.metrics
    }
    pub fn path(&self) -> &str {
        &self.path
    }
    pub fn hypothesis(&self) -> Option<UnitConversion> {
        self.hypothesis
    }

    /// Stamp evidence only; callers retain final §4 policy and publication.
    /// Stale patch/proof claims cannot survive a diagnosis-only finding.
    pub fn audit(&self, mut draft: AuditArtifact, side: Side) -> Result<AuditArtifact, Error> {
        draft.drift = Some(Drift {
            class: DriftClass::Semantic,
            side: match side {
                Side::Request => DriftSide::Request,
                Side::Response => DriftSide::Response,
            },
            changes: vec![Change {
                path: self
                    .path
                    .clone()
                    .try_into()
                    .map_err(|_| Error::AuditConversion)?,
                kind: "magnitude_changed"
                    .try_into()
                    .map_err(|_| Error::AuditConversion)?,
                from: None,
                to: None,
            }],
            vendor: None,
            first_seen: None,
            occurrences: None,
        });
        draft.hypothesis = None;
        draft.detail = None;
        draft.note = None;
        draft.confidence = None;
        draft.patch = None;
        draft.proof.write_integrity = false;
        if let Some(h) = self.hypothesis {
            draft.hypothesis = Some(
                "unit_conversion"
                    .try_into()
                    .map_err(|_| Error::AuditConversion)?,
            );
            draft.detail = Some(
                format!(
                    "{}: median {} → {}, ratio {} (±2%)",
                    self.path, h.baseline_median, h.observed_median, h.ratio
                )
                .try_into()
                .map_err(|_| Error::AuditConversion)?,
            );
        }
        Ok(draft)
    }
}

fn median(values: &[f64]) -> f64 {
    let mut values = values.to_vec();
    values.sort_by(f64::total_cmp);
    let i = values.len() / 2;
    if values.len().is_multiple_of(2) {
        let (a, b) = (values[i - 1], values[i]);
        // Preserve tiny equal values, while avoiding overflow at large values.
        if a.signum() == b.signum() {
            a + (b - a) / 2.
        } else {
            a / 2. + b / 2.
        }
    } else {
        values[i]
    }
}

pub fn detect(path: &str, baseline: &[f64], observed: &[f64]) -> Result<Detection, Error> {
    if path.is_empty() {
        return Err(Error::InvalidInput);
    }
    if path.len() > 4096 {
        return Err(Error::ResourceLimit);
    }
    metrics::validate(baseline, observed)?;
    if baseline.len() < 20 || observed.len() < 20 {
        return Ok(Detection::InsufficientEvidence);
    }
    let metrics = measure(baseline, observed)?;
    if metrics.psi < 0.2 || metrics.normalized_wasserstein < 0.2 {
        return Ok(Detection::Stable(metrics));
    }
    let (a, b) = (median(baseline), median(observed));
    let hypothesis = if a == 0. {
        None
    } else {
        let ratio = b / a;
        if !ratio.is_finite() {
            return Err(Error::NumericRange);
        }
        [
            Conversion::Times100,
            Conversion::Divide100,
            Conversion::Times1000,
            Conversion::Divide1000,
        ]
        .into_iter()
        .find(|c| (c.factor() * 0.98..=c.factor() * 1.02).contains(&ratio))
        .map(|conversion| UnitConversion {
            baseline_median: a,
            observed_median: b,
            ratio,
            conversion,
            tolerance: 0.02,
        })
    };
    Ok(Detection::Drift(Finding {
        path: path.into(),
        metrics,
        hypothesis,
    }))
}
