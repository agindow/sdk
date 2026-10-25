//! TI-23: explicit quantity identity, bounded workflow observations and candidates.
pub mod discovery;
mod store;
use crate::valid_text;
pub use store::{Limits, Snapshot, Store};

pub const MAX_RULES: usize = 16;
pub const MAX_SOURCES: usize = 32;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Invalid,
    Capacity,
    Busy,
    Poisoned,
    Stale,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Request,
    Response,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    pub operation: String,
    pub side: Side,
    pub field: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scope {
    pub tenant: String,
    pub connection: String,
    pub vendor: String,
    pub workflow: String,
    pub entity: String,
    pub revision: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Key {
    pub endpoint: Endpoint,
    pub concept: String,
    pub unit: String,
    pub contract_version: u64,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Observation {
    pub scope: Scope,
    pub key: Key,
    pub value: Option<i64>,
    pub at_ms: i64,
    pub sequence: u64,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    pub id: String,
    pub left: Endpoint,
    pub right: Endpoint,
    pub concept: String,
    pub unit: String,
    pub tolerance: u64,
    pub max_age_ms: u64,
    pub max_skew_ms: u64,
}
impl Endpoint {
    pub(crate) fn valid(&self) -> bool {
        valid_text(&self.operation) && valid_text(&self.field)
    }
}
impl Scope {
    pub fn validate(&self) -> Result<(), Error> {
        if [
            &self.tenant,
            &self.connection,
            &self.vendor,
            &self.workflow,
            &self.entity,
            &self.revision,
        ]
        .iter()
        .all(|s| valid_text(s))
        {
            Ok(())
        } else {
            Err(Error::Invalid)
        }
    }
}
impl Key {
    pub fn validate(&self) -> Result<(), Error> {
        if self.endpoint.valid()
            && valid_text(&self.concept)
            && valid_text(&self.unit)
            && self.contract_version > 0
        {
            Ok(())
        } else {
            Err(Error::Invalid)
        }
    }
}
impl Rule {
    pub fn validate(&self) -> Result<(), Error> {
        if !valid_text(&self.id)
            || !self.left.valid()
            || !self.right.valid()
            || self.left.operation == self.right.operation
            || !valid_text(&self.concept)
            || !valid_text(&self.unit)
            || self.max_age_ms == 0
            || self.max_age_ms > 86_400_000
            || self.max_skew_ms > self.max_age_ms
        {
            return Err(Error::Invalid);
        }
        Ok(())
    }
    pub fn key(&self, endpoint: &Endpoint, contract_version: u64) -> Key {
        Key {
            endpoint: endpoint.clone(),
            concept: self.concept.clone(),
            unit: self.unit.clone(),
            contract_version,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    Pass,
    Divergence,
    Partial,
    ScopeMismatch,
    Unsupported,
    Stale,
    Future,
    Skew,
    OutOfOrder,
}
/// Compare only qualified, contemporary evidence. Arithmetic is exact in i128.
pub fn compare(
    rule: &Rule,
    left: Option<&Observation>,
    right: Option<&Observation>,
    now_ms: i64,
) -> Result<Reason, Error> {
    rule.validate()?;
    let (Some(a), Some(b)) = (left, right) else {
        return Ok(Reason::Partial);
    };
    a.scope.validate()?;
    b.scope.validate()?;
    a.key.validate()?;
    b.key.validate()?;
    if a.sequence == 0 || b.sequence == 0 {
        return Err(Error::Invalid);
    }
    if a.scope != b.scope
        || a.key.endpoint != rule.left
        || b.key.endpoint != rule.right
        || a.key.concept != rule.concept
        || b.key.concept != rule.concept
        || a.key.unit != rule.unit
        || b.key.unit != rule.unit
        || a.key.contract_version != b.key.contract_version
    {
        return Ok(Reason::ScopeMismatch);
    }
    let (Some(av), Some(bv)) = (a.value, b.value) else {
        return Ok(Reason::Unsupported);
    };
    let age_a = i128::from(now_ms) - i128::from(a.at_ms);
    let age_b = i128::from(now_ms) - i128::from(b.at_ms);
    if age_a < 0 || age_b < 0 {
        return Ok(Reason::Future);
    }
    if age_a > i128::from(rule.max_age_ms) || age_b > i128::from(rule.max_age_ms) {
        return Ok(Reason::Stale);
    }
    if (i128::from(a.at_ms) - i128::from(b.at_ms)).abs() > i128::from(rule.max_skew_ms) {
        return Ok(Reason::Skew);
    }
    Ok(
        if (i128::from(av) - i128::from(bv)).abs() <= i128::from(rule.tolerance) {
            Reason::Pass
        } else {
            Reason::Divergence
        },
    )
}
