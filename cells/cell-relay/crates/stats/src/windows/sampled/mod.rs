//! TI-51 opt-in uniform call sampling; runtime contract §10.28.
//! Caller entropy and statistical assumptions are explicit; no provider or clock.
mod evidence;
mod store;
use super::{Projection, Scope, Window};
use agw_inspector::Value;
pub use evidence::{Assumptions, Evidence};
use std::sync::Arc;
pub use store::Store;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Policy {
    /// Caller-owned version; never reused for a changed sampling design.
    pub version: String,
    pub max_items: usize,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    pub scope: Scope,
    /// Object-only pointer from document root to the array.
    pub array: String,
    /// Object-only pointer from selected object to a scalar/missing leaf.
    pub item: String,
    pub policy: Policy,
    pub projection: Projection,
}
#[derive(Clone, Copy)]
pub struct Draw<'a> {
    pub selection: &'a Selection,
    /// Uniform independent caller word. Rejected low words admit no sample.
    pub entropy: u64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SamplingUnit {
    EligibleCall,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Window(super::Error),
    InvalidPolicy,
    NoOpportunity,
    EntropyRejected,
    UnsupportedItems,
    UnsupportedAssumptions,
    ProjectionUnavailable,
    Significance(crate::significance::Error),
}
impl From<super::Error> for Error {
    fn from(error: super::Error) -> Self {
        Self::Window(error)
    }
}
impl From<crate::significance::Error> for Error {
    fn from(error: crate::significance::Error) -> Self {
        Self::Significance(error)
    }
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}

#[derive(Debug, Clone)]
pub struct Snapshot {
    selection: Arc<Selection>,
    inner: super::Snapshot,
}
impl Snapshot {
    pub fn selection(&self) -> &Selection {
        &self.selection
    }
    pub fn unit(&self) -> SamplingUnit {
        SamplingUnit::EligibleCall
    }
    pub fn baseline(&self) -> &Arc<Window> {
        self.inner.baseline()
    }
    pub fn live(&self) -> &Arc<Window> {
        self.inner.live()
    }
    pub fn evidence(&self, assumptions: Assumptions) -> Result<Evidence<'_>, Error> {
        if !assumptions.independent_exchangeable_calls || !assumptions.comparable_populations {
            return Err(Error::UnsupportedAssumptions);
        }
        Ok(Evidence { snapshot: self })
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
impl Selection {
    fn key(&self) -> Result<super::Selection, Error> {
        self.scope.bytes()?;
        if self.policy.version.is_empty()
            || self.policy.version.len() > 128
            || !(1..=4096).contains(&self.policy.max_items)
        {
            return Err(Error::InvalidPolicy);
        }
        // Validate every pointer even when the document/array is absent.
        super::extract::node(&Value::Object(Vec::new()), &self.array)?;
        super::extract::node(&Value::Object(Vec::new()), &self.item)?;
        let mut scope = self.scope.clone();
        // Canonical tuple encoding prevents delimiter collisions and keeps the
        // shared store's scope/sample/byte quotas atomic across all policies.
        scope.field = serde_json::to_string(&(
            "uniform-call-v1",
            &self.scope.field,
            &self.array,
            &self.item,
            &self.policy.version,
            self.policy.max_items,
        ))
        .map_err(|_| Error::InvalidPolicy)?;
        scope.bytes()?;
        Ok(super::Selection {
            scope,
            projection: self.projection,
        })
    }
    fn sample<'a>(
        &self,
        root: &'a Value,
        entropy: u64,
        work: &mut usize,
    ) -> Result<Option<&'a Value>, Error> {
        let values = match super::extract::node_work(root, &self.array, work)? {
            None | Some(Value::Null) => return Err(Error::NoOpportunity),
            Some(Value::Array(values)) if values.is_empty() => return Err(Error::NoOpportunity),
            Some(Value::Array(values)) => values,
            _ => return Err(Error::UnsupportedItems),
        };
        if values.len() > self.policy.max_items {
            return Err(super::Error::ResourceLimit.into());
        }
        *work = work
            .checked_sub(values.len())
            .ok_or(super::Error::ResourceLimit)?;
        if values.iter().any(|v| !matches!(v, Value::Object(_))) {
            return Err(Error::UnsupportedItems);
        }
        let n = values.len() as u64;
        // Exactly 2^64 mod n words are discarded, making the accepted domain
        // divisible by n. No loop or external randomness is needed here.
        if entropy < n.wrapping_neg() % n {
            return Err(Error::EntropyRejected);
        }
        let item = values
            .get((entropy % n) as usize)
            .ok_or(Error::InvalidPolicy)?;
        Ok(super::extract::scalar_work(item, &self.item, work)?)
    }
}
