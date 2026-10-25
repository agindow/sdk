//! TI-27 local replay safety, runtime contract §§5 and 10.37.
//! Explicit native boundary; no remote coordination or automatic retries.
mod forwarding;
mod gated;
mod store;
pub use forwarding::{Bindings, Forwarding};
pub use gated::Completion;
pub use store::Store;

use crate::feedback::ErrorCode;
use agw_policy::Operation;
use std::{fmt, sync::Arc, time::Duration};

#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub entries: usize,
    /// Starts at completion; never expires an operation still running.
    pub window: Duration,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            entries: 1024,
            window: Duration::from_secs(86400),
        }
    }
}
#[derive(Debug, Clone, Copy)]
pub struct Request<'a> {
    pub connection: &'a str,
    pub operation: Operation,
    pub key: Option<&'a str>,
    /// Opaque provider hint, interpreted only by the configured upstream.
    pub durability: Option<&'a str>,
}
impl Request<'_> {
    fn validate(self) -> Result<(), Rejection> {
        if !scope(self.connection) {
            return Err(Rejection::invalid("invalid connection"));
        }
        if (self.operation != Operation::Read && self.key.is_none())
            || self.key.is_some_and(|key| !opaque(key, 256))
        {
            return Err(Rejection::invalid(
                "a valid idempotency_key is required for writes/unknown operations",
            ));
        }
        if self.durability.is_some_and(|hint| !opaque(hint, 128)) {
            return Err(Rejection::invalid("invalid durability hint"));
        }
        Ok(())
    }
}
fn scope(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 256 && !value.chars().any(char::is_control)
}
fn opaque(value: &str, limit: usize) -> bool {
    !value.is_empty()
        && value.len() <= limit
        && value.bytes().all(|byte| (33..=126).contains(&byte))
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DurabilityStatus {
    NotRequested,
    Forwarded,
    Unsupported,
}
impl DurabilityStatus {
    pub fn warning(self) -> Option<&'static str> {
        (self == Self::Unsupported)
            .then_some("durability hint unsupported: no upstream binding configured")
    }
}
/// Shared original outcome; no `Clone` bound or payload reconstruction.
#[derive(Debug)]
pub struct Replay<T> {
    pub value: Arc<T>,
    pub replayed: bool,
    pub durability: DurabilityStatus,
}
#[derive(Debug)]
#[must_use = "Honor admission and indeterminate outcomes; never blindly dispatch again"]
pub enum Access<T> {
    Completed(Replay<T>),
    /// Same-thread recursive request; waiting would deadlock the original caller.
    InProgress,
    /// The caller unwound before recording an outcome; the write may have happened.
    Indeterminate,
}
/// Deterministic envelope errors or configured capacity policy, not engine faults.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rejection {
    pub code: ErrorCode,
    pub reason: &'static str,
}
impl Rejection {
    pub fn http_status(&self) -> u16 {
        self.code.http_status()
    }
    pub(crate) fn invalid(reason: &'static str) -> Self {
        Self {
            code: ErrorCode::InvalidRequest,
            reason,
        }
    }
    pub(crate) fn full() -> Self {
        Self {
            code: ErrorCode::RateLimited,
            reason: "local replay capacity exhausted; unexpired writes retained",
        }
    }
}
impl fmt::Display for Rejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.reason)
    }
}
impl std::error::Error for Rejection {}
