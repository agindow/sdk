//! TI-11 shape diff and TI-13 standalone statistical rename evidence.
//!
//! No policy decisions, payload rewriting, I/O or baseline selection. Learned
//! presence is not requiredness or repair confidence. Semantic metadata and
//! confirmed identities come from callers; `rename` supplies distribution evidence.
//! See runtime contract §10.13 for the evidence and inspection boundaries.
pub mod class;
mod compare;
mod inspection;
mod model;
pub mod rename;
mod validate;

pub use class::DriftClass;
pub use compare::{compare, compare_with_limits};
pub use model::{Change, Evidence, Kind, Limits, Node, Reason, Report, Side, Skip, Snapshot};

pub use inspection::{compare_inspection, compare_inspection_with_limits};
