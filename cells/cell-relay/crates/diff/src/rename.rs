//! TI-13: bounded distribution evidence, independent of names and repair policy.
//! Callers own scope/window selection and collision-free, compatible binning.
//! See runtime contract §10.15; confidence is a score, not calibration.
use agw_inspector::inference::JsonType;
use std::collections::{BTreeMap, BTreeSet};

const MAX_FIELDS: usize = 64;
const MAX_BINS: usize = 256;
const MAX_TEXT: usize = 1024 * 1024;
const PSI_LIMIT: f64 = 0.1;
const JS_LIMIT: f64 = 0.02;

/// A caller-selected removed/added sibling. Counts exclude absent observations;
/// opportunities include them. Exact labels must retain scalar type semantics.
#[derive(Debug, Clone)]
pub struct Field {
    pub name: String,
    pub kind: JsonType,
    pub binning: String,
    pub counts: BTreeMap<String, u64>,
    pub opportunities: u64,
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Distances {
    pub psi: f64,
    pub js: f64,
}
#[derive(Debug, Clone, PartialEq)]
pub enum Decision {
    NoMatch,
    Ambiguous,
    Confirmed {
        added: usize,
        distances: Distances,
        confidence: f64,
    },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    InvalidInput,
    ResourceLimit,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}

fn validate(field: &Field, text: &mut usize) -> Result<u64, Error> {
    if field.counts.len() > MAX_BINS {
        return Err(Error::ResourceLimit);
    }
    for value in std::iter::once(&field.name)
        .chain(std::iter::once(&field.binning))
        .chain(field.counts.keys())
    {
        *text = text.saturating_add(value.len());
        if *text > MAX_TEXT {
            return Err(Error::ResourceLimit);
        }
    }
    let total = field.counts.values().try_fold(0_u64, |sum, n| {
        if *n == 0 {
            return Err(Error::InvalidInput);
        }
        sum.checked_add(*n).ok_or(Error::InvalidInput)
    })?;
    if total > field.opportunities {
        return Err(Error::InvalidInput);
    }
    Ok(total)
}
fn comparable(a: &Field, b: &Field, na: u64, nb: u64) -> bool {
    na > 0
        && nb > 0
        && a.kind == b.kind
        && a.binning == b.binning
        && matches!(
            a.kind,
            JsonType::String | JsonType::Integer | JsonType::Number | JsonType::Boolean
        )
}

/// Validated pair metrics. `None` means incompatible or no usable evidence.
/// PSI floors missing support; JS keeps zero probabilities exactly zero.
pub fn distances(a: &Field, b: &Field) -> Result<Option<Distances>, Error> {
    let mut text = 0;
    let na = validate(a, &mut text)?;
    let nb = validate(b, &mut text)?;
    Ok(comparable(a, b, na, nb).then(|| measure(a, b, na, nb)))
}
fn measure(a: &Field, b: &Field, na: u64, nb: u64) -> Distances {
    let keys: BTreeSet<_> = a.counts.keys().chain(b.counts.keys()).collect();
    let probabilities: Vec<_> = keys
        .iter()
        .map(|key| {
            (
                a.counts.get(*key).copied().unwrap_or(0) as f64 / na as f64,
                b.counts.get(*key).copied().unwrap_or(0) as f64 / nb as f64,
            )
        })
        .collect();
    let floor = 1e-6;
    let sa: f64 = probabilities.iter().map(|(p, _)| p.max(floor)).sum();
    let sb: f64 = probabilities.iter().map(|(_, q)| q.max(floor)).sum();
    let mut result = Distances { psi: 0.0, js: 0.0 };
    for (p, q) in probabilities {
        let m = (p + q) / 2.0;
        if p > 0.0 {
            result.js += 0.5 * p * (p / m).ln();
        }
        if q > 0.0 {
            result.js += 0.5 * q * (q / m).ln();
        }
        let pp = p.max(floor) / sa;
        let qq = q.max(floor) / sb;
        result.psi += (pp - qq) * (pp.ln() - qq.ln());
    }
    // Roundoff at identity must never create negative distance/confidence >1.
    result.psi = result.psi.max(0.0);
    result.js = result.js.max(0.0);
    result
}

/// One decision per removed field, in caller order. Matching is simultaneous:
/// every plausible edge counts in both directions before any score is applied.
/// Invalid input aborts the entire operation; no partial identities are exposed.
pub fn match_fields(removed: &[Field], added: &[Field]) -> Result<Vec<Decision>, Error> {
    if removed.len() > MAX_FIELDS || added.len() > MAX_FIELDS {
        return Err(Error::ResourceLimit);
    }
    let mut text = 0;
    let mut names = BTreeSet::new();
    let mut counts = Vec::new();
    for field in removed.iter().chain(added) {
        counts.push(validate(field, &mut text)?);
        if !names.insert(&field.name) {
            return Err(Error::InvalidInput);
        }
    }
    let mut edges = vec![Vec::new(); removed.len()];
    let mut incoming = vec![0; added.len()];
    for (i, a) in removed.iter().enumerate() {
        for (j, b) in added.iter().enumerate() {
            let na = counts[i];
            let nb = counts[removed.len() + j];
            if !comparable(a, b, na, nb) {
                continue;
            }
            let d = measure(a, b, na, nb);
            if d.psi <= PSI_LIMIT && d.js <= JS_LIMIT {
                edges[i].push((j, d));
                incoming[j] += 1;
            }
        }
    }
    Ok(edges
        .iter()
        .enumerate()
        .map(|(i, edge)| match edge.as_slice() {
            [] => Decision::NoMatch,
            [(j, d)] if incoming[*j] == 1 => {
                let na = counts[i];
                let nb = counts[removed.len() + j];
                let coverage = (na as f64 / removed[i].opportunities as f64)
                    .min(nb as f64 / added[*j].opportunities as f64);
                let volume = (na.min(nb) as f64 / 100.0).min(1.0);
                let distance = (d.psi / PSI_LIMIT).max(d.js / JS_LIMIT);
                Decision::Confirmed {
                    added: *j,
                    distances: *d,
                    confidence: ((1.0 - 0.1 * distance)
                        * (0.9 + 0.1 * volume)
                        * (0.9 + 0.1 * coverage))
                        .clamp(0.0, 1.0),
                }
            }
            _ => Decision::Ambiguous,
        })
        .collect())
}
