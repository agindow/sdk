//! TI-12: confidence-gated selective repair over captured bytes and their tee.
//!
//! Pure library seams: callers supply drift/identity/value evidence and own
//! final policy/proof. No network, registry, guard wiring or payload rewriting
//! outside explicitly targeted tokens. See runtime contract §10.14.
pub mod coercion;
mod edits;
mod engine;
mod model;
mod path;
mod planning;
mod raw;
mod spans;

pub use coercion::{JsonType, Proposal, Refusal, check};
pub use model::{
    Assessment, Candidate, Evidence, Failure, Limits, Mode, Operation, Outcome, Reject, Status,
};
pub use planning::evaluate;

pub use engine::{repair, repair_with_limits};
