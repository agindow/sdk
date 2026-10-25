//! TI-45: local, explicitly published statistical generations (contract §10.26).
//! No callbacks, clocks, transport or guard decisions execute here.
mod extract;
pub mod projection;
pub mod sampled;
mod store;
use crate::enumeration;
use agw_diff::Side;
use agw_inspector::{Precision, Value};
use chrono::{DateTime, Utc};
use std::sync::Arc;
pub use store::Store;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scope {
    pub tenant: String,
    pub connection: String,
    pub vendor: String,
    pub operation: String,
    pub side: Side,
    /// Strict JSON Pointer with object-property traversal only.
    pub field: String,
    pub segment: String,
    pub window: String,
    pub generation: String,
}
// Side's public shape comparator API deliberately promises equality only.
impl Ord for Scope {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.parts().cmp(&other.parts()).then_with(|| {
            matches!(self.side, Side::Response).cmp(&matches!(other.side, Side::Response))
        })
    }
}
impl PartialOrd for Scope {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Scope {
    fn parts(&self) -> [&str; 8] {
        [
            &self.tenant,
            &self.connection,
            &self.vendor,
            &self.operation,
            &self.field,
            &self.segment,
            &self.window,
            &self.generation,
        ]
    }
    pub(super) fn bytes(&self) -> Result<usize, Error> {
        self.parts()
            .into_iter()
            .try_fold(std::mem::size_of::<Self>(), |sum, part| {
                if part.is_empty() || part.len() > 4096 {
                    return Err(Error::InvalidInput);
                }
                sum.checked_add(part.len()).ok_or(Error::ResourceLimit)
            })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Projection {
    Disabled,
    /// Caller must propagate the selected field's precision hint. Only Observed
    /// integers within ±(2^53−1) project; retained lexemes never do.
    ExactInteger(Precision),
    /// Explicit provenance-retaining decimal materialization (TI-52).
    Decimal(projection::Policy),
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    pub scope: Scope,
    pub projection: Projection,
}
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub samples: usize,
    pub scopes: usize,
    pub bytes: usize,
    pub batch: usize,
    pub inspection_nodes: usize,
    pub inspection_bytes: usize,
    pub minimum: usize,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    InvalidInput,
    ResourceLimit,
    Incomplete,
    DuplicateKey,
    UnsupportedPath,
    UnknownScope,
    InsufficientSamples,
    AlreadyPublished,
    ProviderUnavailable,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sample {
    value: Option<Value>,
    at: DateTime<Utc>,
}
impl Sample {
    pub fn value(&self) -> Option<&Value> {
        self.value.as_ref()
    }
    pub fn at(&self) -> DateTime<Utc> {
        self.at
    }
    pub(super) fn bytes(value: Option<&Value>) -> usize {
        std::mem::size_of::<Self>()
            + match value {
                Some(Value::String(s)) => s.len(),
                Some(Value::Number(n)) => n.lexeme_len(),
                _ => 0,
            }
    }
}
#[derive(Debug, Clone)]
pub struct Window {
    samples: Vec<Sample>,
    pub(crate) projection: Projection,
}
impl Window {
    pub fn samples(&self) -> &[Sample] {
        &self.samples
    }
    pub fn fields(&self) -> Vec<Option<&Value>> {
        self.samples.iter().map(Sample::value).collect()
    }
    pub fn enumeration(&self) -> Vec<enumeration::Observation<'_>> {
        self.samples
            .iter()
            .filter_map(|s| match s.value() {
                Some(Value::String(value)) => Some(enumeration::Observation { value, at: s.at }),
                _ => None,
            })
            .collect()
    }
    pub fn vocabulary(&self) -> Vec<&str> {
        self.enumeration()
            .iter()
            .map(|o| o.value)
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect()
    }
    /// None means unit analysis is unavailable for this window. A single unsafe
    /// numeric sample disables it, avoiding a biased subset of numeric traffic.
    pub fn unit(&self) -> Option<Vec<f64>> {
        if self.projection != Projection::ExactInteger(Precision::Observed) {
            return None;
        }
        let mut values = Vec::new();
        for sample in &self.samples {
            if let Some(Value::Number(number)) = sample.value() {
                use agw_inspector::Number;
                let value = match number {
                    Number::I64(n) if n.unsigned_abs() <= 9_007_199_254_740_991 => *n as f64,
                    Number::U64(n) if *n <= 9_007_199_254_740_991 => *n as f64,
                    _ => return None,
                };
                values.push(value);
            }
        }
        (!values.is_empty()).then_some(values)
    }
    pub(super) fn bytes(&self) -> usize {
        self.samples
            .iter()
            .map(|s| Sample::bytes(s.value()))
            .sum::<usize>()
            + std::mem::size_of::<Self>()
    }
}
#[derive(Debug, Clone)]
pub struct Snapshot {
    scope: Arc<Scope>,
    baseline: Arc<Window>,
    live: Arc<Window>,
    accounted_bytes: usize,
}
impl Snapshot {
    pub fn scope(&self) -> &Scope {
        &self.scope
    }
    pub fn baseline(&self) -> &Arc<Window> {
        &self.baseline
    }
    pub fn live(&self) -> &Arc<Window> {
        &self.live
    }
}
#[derive(Debug, Clone)]
pub enum Availability {
    Unavailable(Error),
    Warming { baseline: usize, live: usize },
    Ready(Snapshot),
}
impl Availability {
    pub fn ready(self) -> Option<Snapshot> {
        match self {
            Self::Ready(s) => Some(s),
            _ => None,
        }
    }
}

impl Snapshot {
    /// Accounted retained bytes, for precharging runtime materialization. Does
    /// not include allocator slack or scan/copy the retained strings.
    pub fn accounted_bytes(&self) -> usize {
        self.accounted_bytes
    }
    /// Transient current-call opportunity; never mutates/publishes the store.
    /// Caller must precharge extraction/copy costs before entering this method.
    pub fn with_current(
        &self,
        inspection: &agw_inspector::Inspection,
        at: DateTime<Utc>,
        limits: Limits,
    ) -> Result<Window, Error> {
        if limits.samples == 0 || limits.samples > 4096 {
            return Err(Error::InvalidInput);
        }
        let root = extract::complete(inspection, limits)?;
        let value = extract::scalar(root, &self.scope.field)?;
        let bytes = self
            .live
            .bytes()
            .checked_add(Sample::bytes(value))
            .ok_or(Error::ResourceLimit)?;
        if bytes > limits.bytes || Sample::bytes(value) > 4096 + std::mem::size_of::<Sample>() {
            return Err(Error::ResourceLimit);
        }
        let skip = self
            .live
            .samples
            .len()
            .saturating_add(1)
            .saturating_sub(limits.samples);
        let mut samples: Vec<_> = self.live.samples.iter().skip(skip).cloned().collect();
        samples.push(Sample {
            value: value.cloned(),
            at,
        });
        Ok(Window {
            samples,
            projection: self.live.projection,
        })
    }
}

/// Complete, duplicate-free bounded input validation, without store mutation.
/// Runtime callers precharge traversal before invoking this helper.
pub fn validate_inspection(
    inspection: &agw_inspector::Inspection,
    limits: Limits,
) -> Result<(), Error> {
    extract::complete(inspection, limits).map(|_| ())
}
