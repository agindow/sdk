//! TI-49: selected aggregate snapshots. Background owners drive refresh jobs;
//! synchronous reads have no transport/storage/provider callback.
mod cache;
#[cfg(not(target_arch = "wasm32"))]
pub mod native;
mod wire;
pub use cache::{Cache, Job, Limits, Metrics, Telemetry};
pub use wire::{Grain, Scope, Selected, Side, Snapshot};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Invalid,
    Capacity,
    Regression,
    Conflict,
    Obsolete,
    Timeout,
    Transport,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}
