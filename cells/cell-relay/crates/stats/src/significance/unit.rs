use super::{Error, Hypothesis, Report, Test, magnitude, report};
use crate::{control::Signal, unit as detector};

/// Raw f64 projections have the same losslessness obligations as TI-15.
pub fn unit(
    path: &str,
    baseline: &[f64],
    observed: &[f64],
    total_tests: usize,
) -> Result<Report<detector::Detection>, Error> {
    report::family(total_tests, 1)?;
    report::text(path, std::iter::empty())?;
    let detection = detector::detect(path, baseline, observed).map_err(Error::Unit)?;
    let sufficient = !matches!(detection, detector::Detection::InsufficientEvidence);
    let tests = if sufficient {
        vec![Test::new(
            Hypothesis::Magnitude,
            magnitude(baseline, observed)?,
            Signal::UnitHypothesis,
            total_tests,
            matches!(detection, detector::Detection::Drift(_)),
        )]
    } else {
        Vec::new()
    };
    Ok(Report::new(path, detection, sufficient, tests))
}
