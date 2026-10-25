use super::{Error, Observation, Policy, checked, math::State};
use crate::{
    unit,
    windows::{Projection, Window},
};
use agw_inspector::Value;
use chrono::{DateTime, Utc};
#[derive(Debug, Clone)]
pub struct NumericSample {
    index: usize,
    at: DateTime<Utc>,
    projection: Observation,
}
impl NumericSample {
    pub fn index(&self) -> usize {
        self.index
    }
    pub fn at(&self) -> DateTime<Utc> {
        self.at
    }
    pub fn projection(&self) -> &Observation {
        &self.projection
    }
}
#[derive(Debug, Clone)]
pub struct UnitWindow {
    policy: Policy,
    values: Vec<f64>,
    samples: Vec<NumericSample>,
}
impl UnitWindow {
    pub fn policy(&self) -> Policy {
        self.policy
    }
    pub fn values(&self) -> &[f64] {
        &self.values
    }
    pub fn samples(&self) -> &[NumericSample] {
        &self.samples
    }
}
impl Window {
    /// Explicit materialization retaining authorization and exact error proof.
    /// A rejected numeric sample invalidates the entire unit window.
    pub fn projected_unit(&self) -> Result<UnitWindow, Error> {
        let Projection::Decimal(policy) = self.projection else {
            return Err(Error::Unauthorized);
        };
        let mut state = State::new(policy.limits());
        state.work(self.samples().len())?;
        let count = self
            .samples()
            .iter()
            .filter(|s| matches!(s.value(), Some(Value::Number(_))))
            .count();
        if count == 0 {
            return Err(Error::NoNumeric);
        }
        state.work(self.samples().len())?;
        let retained = count
            .checked_mul(std::mem::size_of::<NumericSample>() + std::mem::size_of::<f64>())
            .and_then(|bytes| bytes.checked_add(std::mem::size_of::<UnitWindow>()))
            .ok_or(Error::ResourceLimit)?;
        state.output(retained)?;
        let mut result = UnitWindow {
            policy,
            values: Vec::with_capacity(count),
            samples: Vec::with_capacity(count),
        };
        for (index, sample) in self.samples().iter().enumerate() {
            if let Some(Value::Number(number)) = sample.value() {
                let projection = checked(number, policy, &mut state)?;
                result.values.push(projection.value());
                result.samples.push(NumericSample {
                    index,
                    at: sample.at(),
                    projection,
                });
            }
        }
        Ok(result)
    }
}
#[derive(Debug, Clone)]
pub struct Comparison {
    baseline: UnitWindow,
    observed: UnitWindow,
    detection: unit::Detection,
}
impl Comparison {
    pub fn baseline(&self) -> &UnitWindow {
        &self.baseline
    }
    pub fn observed(&self) -> &UnitWindow {
        &self.observed
    }
    pub fn detection(&self) -> &unit::Detection {
        &self.detection
    }
}
/// The detector remains descriptive. Both source projections/proofs accompany
/// its result; no repair candidate or inferred permission is produced.
pub fn compare(path: &str, baseline: &Window, observed: &Window) -> Result<Comparison, Error> {
    if baseline.projection != observed.projection {
        return Err(Error::IncompatiblePolicies);
    }
    let baseline = baseline.projected_unit()?;
    let observed = observed.projected_unit()?;
    let detection =
        unit::detect(path, baseline.values(), observed.values()).map_err(Error::Detector)?;
    Ok(Comparison {
        baseline,
        observed,
        detection,
    })
}
