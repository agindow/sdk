//! Offline candidates from complete, explicitly correlated profiles. Never active rules.
use super::{Endpoint, Error, MAX_RULES, Rule};
use crate::valid_text;
#[derive(Debug, Clone)]
pub struct Sample {
    pub correlation: String,
    pub value: Option<i64>,
    pub at_ms: i64,
}
#[derive(Debug, Clone)]
pub struct Profile {
    pub endpoint: Endpoint,
    pub concept: String,
    pub unit: String,
    pub complete: bool,
    pub samples: Vec<Sample>,
}
#[derive(Debug, Clone, Copy)]
pub struct Policy {
    pub minimum_pairs: usize,
    pub tolerance: u64,
    pub max_age_ms: u64,
    pub max_skew_ms: u64,
}
/// Bounds: 16 fields, 64 samples/field, 16 candidates. Full paired coverage and
/// variation on both sides prevent constant/marginal matches being sold as proof.
pub fn discover(profiles: &[Profile], policy: Policy) -> Result<Vec<Rule>, Error> {
    if profiles.len() > 16 {
        return Err(Error::Capacity);
    }
    if !(3..=64).contains(&policy.minimum_pairs)
        || policy.max_age_ms == 0
        || policy.max_age_ms > 86_400_000
        || policy.max_skew_ms > policy.max_age_ms
    {
        return Err(Error::Invalid);
    }
    for (i, p) in profiles.iter().enumerate() {
        if !p.endpoint.valid()
            || !valid_text(&p.concept)
            || !valid_text(&p.unit)
            || profiles[..i].iter().any(|q| q.endpoint == p.endpoint)
        {
            return Err(Error::Invalid);
        }
        if p.samples.len() > 64 {
            return Err(Error::Capacity);
        }
        for (j, s) in p.samples.iter().enumerate() {
            if !valid_text(&s.correlation)
                || p.samples[..j]
                    .iter()
                    .any(|p| p.correlation == s.correlation)
            {
                return Err(Error::Invalid);
            }
        }
    }
    let mut candidates = Vec::new();
    for (i, a) in profiles.iter().enumerate() {
        for (j, b) in profiles.iter().enumerate().skip(i + 1) {
            if !a.complete
                || !b.complete
                || a.concept != b.concept
                || a.unit != b.unit
                || a.endpoint.operation == b.endpoint.operation
                || a.samples.len() < policy.minimum_pairs
                || a.samples.len() != b.samples.len()
            {
                continue;
            }
            let mut comparable = true;
            for s in &a.samples {
                let Some(peer) = b.samples.iter().find(|p| p.correlation == s.correlation) else {
                    comparable = false;
                    break;
                };
                let (Some(left), Some(right)) = (s.value, peer.value) else {
                    comparable = false;
                    break;
                };
                if (i128::from(left) - i128::from(right)).abs() > i128::from(policy.tolerance)
                    || (i128::from(s.at_ms) - i128::from(peer.at_ms)).abs()
                        > i128::from(policy.max_skew_ms)
                {
                    comparable = false;
                    break;
                }
            }
            let varied = |p: &Profile| {
                p.samples
                    .first()
                    .is_some_and(|first| p.samples.iter().any(|s| s.value != first.value))
            };
            if !comparable || !varied(a) || !varied(b) {
                continue;
            }
            if candidates.len() >= MAX_RULES {
                return Err(Error::Capacity);
            }
            candidates.push(Rule {
                id: format!("cross-{i}-{j}"),
                left: a.endpoint.clone(),
                right: b.endpoint.clone(),
                concept: a.concept.clone(),
                unit: a.unit.clone(),
                tolerance: policy.tolerance,
                max_age_ms: policy.max_age_ms,
                max_skew_ms: policy.max_skew_ms,
            });
        }
    }
    Ok(candidates)
}
