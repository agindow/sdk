//! Bounded off-path measurement, never an automatic retry controller. Hosts own
//! one tracker per tenant/measurement arm/window and attest task success.
use serde::Serialize;
use std::{collections::BTreeMap, fmt};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryOutcome {
    Failed,
    Bypassed,
    /// Requires host verification of the task result, not just an HTTP 200.
    VerifiedTaskSuccess,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Receipt {
    Recorded,
    Duplicate,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackingError {
    InvalidCapacity,
    InvalidIdentity,
    Full,
    UnknownBlock,
}
impl fmt::Display for TrackingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "feedback recovery tracking: {self:?}")
    }
}
impl std::error::Error for TrackingError {}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Metrics {
    pub blocked: usize,
    pub retried: usize,
    pub recovered: usize,
}
impl Metrics {
    pub fn recovery_rate(self) -> Option<f64> {
        (self.retried != 0).then(|| self.recovered as f64 / self.retried as f64)
    }
}
#[derive(Default)]
struct State {
    retried: bool,
    recovered: bool,
}
/// Retains finalized entries to deduplicate callbacks. Full windows reject new
/// identities without eviction or changing counters; rotate off-path explicitly.
pub struct Tracker {
    capacity: usize,
    entries: BTreeMap<(String, String), State>,
    metrics: Metrics,
}
impl Tracker {
    pub fn new(capacity: usize) -> Result<Self, TrackingError> {
        if capacity == 0 || capacity > 65_536 {
            return Err(TrackingError::InvalidCapacity);
        }
        Ok(Self {
            capacity,
            entries: BTreeMap::new(),
            metrics: Metrics::default(),
        })
    }
    /// Register only actual policy blocks. Audit/tool scope is supplied by the
    /// host; this API neither verifies remote receipts nor publishes metrics.
    pub fn blocked(&mut self, tool: &str, audit_id: &str) -> Result<Receipt, TrackingError> {
        let key = identity(tool, audit_id)?;
        if self.entries.contains_key(&key) {
            return Ok(Receipt::Duplicate);
        }
        if self.entries.len() == self.capacity {
            return Err(TrackingError::Full);
        }
        self.entries.insert(key, State::default());
        self.metrics.blocked += 1;
        Ok(Receipt::Recorded)
    }
    pub fn retry(
        &mut self,
        tool: &str,
        audit_id: &str,
        outcome: RetryOutcome,
    ) -> Result<Receipt, TrackingError> {
        let key = identity(tool, audit_id)?;
        let state = self
            .entries
            .get_mut(&key)
            .ok_or(TrackingError::UnknownBlock)?;
        let mut changed = false;
        if !state.retried {
            state.retried = true;
            self.metrics.retried += 1;
            changed = true;
        }
        if outcome == RetryOutcome::VerifiedTaskSuccess && !state.recovered {
            state.recovered = true;
            self.metrics.recovered += 1;
            changed = true;
        }
        Ok(if changed {
            Receipt::Recorded
        } else {
            Receipt::Duplicate
        })
    }
    pub fn snapshot(&self) -> Metrics {
        self.metrics
    }
}
fn identity(tool: &str, audit_id: &str) -> Result<(String, String), TrackingError> {
    if [tool, audit_id]
        .iter()
        .any(|s| s.is_empty() || s.len() > 512 || s.chars().any(char::is_control))
    {
        return Err(TrackingError::InvalidIdentity);
    }
    Ok((tool.to_owned(), audit_id.to_owned()))
}
