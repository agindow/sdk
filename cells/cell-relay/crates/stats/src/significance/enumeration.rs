use super::{Error, Hypothesis, Report, Test, fisher, report};
use crate::{control::Signal, enumeration as detector};
use std::collections::BTreeMap;

/// Repeated baseline observations are required: a vocabulary has no sample
/// size. Pooled categories fix the test family before novelty filtering.
pub fn enumeration(
    path: &str,
    baseline: &[&str],
    observed: &[detector::Observation<'_>],
    total_tests: usize,
) -> Result<Report<detector::Report>, Error> {
    report::family(total_tests, 1)?;
    if baseline.len() > 4096 || observed.len() > 4096 {
        return Err(Error::ResourceLimit);
    }
    report::text(
        path,
        baseline
            .iter()
            .map(|x| x.len())
            .chain(observed.iter().map(|x| x.value.len())),
    )?;
    let mut a = BTreeMap::<&str, usize>::new();
    let mut b = BTreeMap::<&str, usize>::new();
    for value in baseline {
        *a.entry(value).or_default() += 1;
        if a.len() > 256 {
            return Err(Error::ResourceLimit);
        }
    }
    for item in observed {
        *b.entry(item.value).or_default() += 1;
        if b.len() > 256 {
            return Err(Error::ResourceLimit);
        }
    }
    let union: std::collections::BTreeSet<_> = a.keys().chain(b.keys()).copied().collect();
    report::family(total_tests, union.len())?;
    let known: Vec<_> = a.keys().copied().collect();
    let detection = detector::compare(path, &known, observed).map_err(Error::Enumeration)?;
    let sufficient = baseline.len() >= 20 && observed.len() >= 20;
    let mut tests = Vec::new();
    if sufficient {
        for value in union {
            let statistic = fisher(
                a.get(value).copied().unwrap_or(0),
                baseline.len(),
                b.get(value).copied().unwrap_or(0),
                observed.len(),
            )?;
            let finding = detection
                .new_values()
                .iter()
                .any(|item| item.value == value && item.material);
            tests.push(Test::new(
                Hypothesis::Category(value.into()),
                statistic,
                Signal::EnumNovelty,
                total_tests,
                finding,
            ));
        }
    }
    Ok(Report::new(path, detection, sufficient, tests))
}
