use super::{Error, Method, Statistic};
use crate::{control, field};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Hypothesis {
    Magnitude,
    Category(String),
    Completeness,
    Sentinel(field::Sentinel),
    Type(field::Type),
    Character(field::CharacterClass),
    Format(field::Format),
}
/// Privately built from the same windows and original detector result.
#[derive(Debug, Clone)]
pub struct Test {
    hypothesis: Hypothesis,
    statistic: Statistic,
    evidence: control::Evidence,
    finding: bool,
}
impl Test {
    pub fn hypothesis(&self) -> &Hypothesis {
        &self.hypothesis
    }
    pub fn method(&self) -> Method {
        self.statistic.method
    }
    pub fn samples(&self) -> (usize, usize) {
        self.statistic.samples
    }
    pub fn evidence(&self) -> control::Evidence {
        self.evidence
    }
    pub fn is_finding(&self) -> bool {
        self.finding
    }
    pub(super) fn new(
        hypothesis: Hypothesis,
        statistic: Statistic,
        signal: control::Signal,
        family: usize,
        finding: bool,
    ) -> Self {
        Self {
            hypothesis,
            statistic,
            finding,
            evidence: control::Evidence {
                signal,
                effect: statistic.effect,
                p_value: statistic.p_value,
                tests: family,
            },
        }
    }
}
#[derive(Debug, Clone)]
pub struct Report<D> {
    path: String,
    detection: D,
    tests: Vec<Test>,
    sufficient: bool,
}
impl<D> Report<D> {
    pub fn path(&self) -> &str {
        &self.path
    }
    pub fn detection(&self) -> &D {
        &self.detection
    }
    pub fn tests(&self) -> &[Test] {
        &self.tests
    }
    pub fn sufficient(&self) -> bool {
        self.sufficient
    }
    /// Local non-findings never become candidates just because p is small.
    /// None is a non-finding; unavailable reports have sufficient=false/no tests.
    pub fn decision(
        &self,
        index: usize,
        floors: &control::Floors<'_>,
        tenant: &str,
    ) -> Result<Option<control::Decision>, control::Error> {
        let test = self.tests.get(index).ok_or(control::Error::InvalidInput)?;
        if !test.finding {
            return Ok(None);
        }
        floors.evaluate(tenant, &self.path, test.evidence).map(Some)
    }
    pub(super) fn new(path: &str, detection: D, sufficient: bool, tests: Vec<Test>) -> Self {
        Self {
            path: path.into(),
            detection,
            sufficient,
            tests,
        }
    }
}
pub(super) fn family(total: usize, local: usize) -> Result<(), Error> {
    if !(1..=4096).contains(&total) || total < local {
        return Err(Error::InvalidInput);
    }
    Ok(())
}
pub(super) fn text(path: &str, values: impl Iterator<Item = usize>) -> Result<(), Error> {
    if path.is_empty() {
        return Err(Error::InvalidInput);
    }
    if path.len() > 1024 {
        return Err(Error::ResourceLimit);
    }
    let mut used = path.len();
    for length in values {
        if length > 4096 {
            return Err(Error::ResourceLimit);
        }
        used += length;
        if used > 1024 * 1024 {
            return Err(Error::ResourceLimit);
        }
    }
    Ok(())
}
