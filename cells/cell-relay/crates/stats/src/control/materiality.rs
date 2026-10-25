use super::{Error, text};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signal {
    MeanShift,
    EnumNovelty,
    UnitHypothesis,
    Completeness,
    TypeConsistency,
    StringFormat,
}
impl Signal {
    pub fn weight(self) -> u8 {
        if self == Self::MeanShift { 1 } else { 2 }
    }
}
/// Evidence supplied by a statistical test, not detector confidence. Effect is
/// an absolute normalized change; conversion/projection belongs to the caller.
#[derive(Debug, Clone, Copy)]
pub struct Evidence {
    pub signal: Signal,
    pub effect: f64,
    pub p_value: f64,
    /// All hypotheses tested, including those which did not produce findings.
    pub tests: usize,
}
#[derive(Debug, Clone, Copy)]
pub struct Override<'a> {
    pub tenant: &'a str,
    pub field: &'a str,
    pub signal: Signal,
    pub floor: f64,
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Decision {
    BelowMateriality,
    NotSignificant,
    Eligible { score: f64 },
}
impl Decision {
    pub fn eligible(self) -> bool {
        matches!(self, Self::Eligible { .. })
    }
}
/// Validated immutable configuration. No implicit per-customer global state.
#[derive(Debug)]
pub struct Floors<'a> {
    alpha: f64,
    agent: f64,
    mean: f64,
    overrides: &'a [Override<'a>],
}
fn unit(value: f64) -> bool {
    value.is_finite() && (0.0..=1.0).contains(&value)
}
fn floor(value: f64) -> bool {
    unit(value) && value > 0.0
}
impl<'a> Floors<'a> {
    pub fn new(
        alpha: f64,
        agent: f64,
        mean: f64,
        overrides: &'a [Override<'a>],
    ) -> Result<Self, Error> {
        if overrides.len() > 256 {
            return Err(Error::ResourceLimit);
        }
        if !floor(alpha) || alpha == 1.0 || !floor(agent) || !floor(mean) {
            return Err(Error::InvalidInput);
        }
        for (index, item) in overrides.iter().enumerate() {
            text(item.tenant)?;
            text(item.field)?;
            if !floor(item.floor)
                || overrides[..index].iter().any(|prior| {
                    (prior.tenant, prior.field, prior.signal)
                        == (item.tenant, item.field, item.signal)
                })
            {
                return Err(Error::InvalidInput);
            }
        }
        Ok(Self {
            alpha,
            agent,
            mean,
            overrides,
        })
    }
    pub fn evaluate(
        &self,
        tenant: &str,
        field: &str,
        evidence: Evidence,
    ) -> Result<Decision, Error> {
        text(tenant)?;
        text(field)?;
        if !unit(evidence.effect)
            || !unit(evidence.p_value)
            || !(1..=4096).contains(&evidence.tests)
        {
            return Err(Error::InvalidInput);
        }
        let threshold = self
            .overrides
            .iter()
            .find(|item| (item.tenant, item.field, item.signal) == (tenant, field, evidence.signal))
            .map(|item| item.floor)
            .unwrap_or(if evidence.signal == Signal::MeanShift {
                self.mean
            } else {
                self.agent
            });
        if evidence.effect < threshold {
            return Ok(Decision::BelowMateriality);
        }
        if evidence.p_value > self.alpha / evidence.tests as f64 {
            return Ok(Decision::NotSignificant);
        }
        Ok(Decision::Eligible {
            score: evidence.effect * f64::from(evidence.signal.weight()),
        })
    }
}

/// Index into caller evidence, ordered by descending weighted materiality.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ranked {
    pub index: usize,
    pub score: f64,
}
impl Floors<'_> {
    pub fn rank(
        &self,
        tenant: &str,
        field: &str,
        items: &[Evidence],
    ) -> Result<Vec<Ranked>, Error> {
        if items.len() > 4096 {
            return Err(Error::ResourceLimit);
        }
        text(tenant)?;
        text(field)?;
        let mut ranked = Vec::new();
        for (index, item) in items.iter().enumerate() {
            if let Decision::Eligible { score } = self.evaluate(tenant, field, *item)? {
                ranked.push(Ranked { index, score });
            }
        }
        ranked.sort_by(|a, b| {
            b.score
                .total_cmp(&a.score)
                .then_with(|| {
                    items[b.index]
                        .signal
                        .weight()
                        .cmp(&items[a.index].signal.weight())
                })
                .then_with(|| a.index.cmp(&b.index))
        });
        Ok(ranked)
    }
}
