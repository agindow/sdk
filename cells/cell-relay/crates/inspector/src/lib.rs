//! Streaming tee and schema inference — inspects bytes without altering them.
//!
//! The tee landed with TI-06: [`Tee`] forwards the upstream body untouched and
//! retains up to 8 MiB of its head; [`inspect`] turns that head into a parse
//! outcome the classifier consumes, with failures recorded as data (contract
//! §4.4). Parsing runs after the stream is driven, never inside a read.
//!
//! TI-07 added [`numeric`]; TI-39 wires it into [`inspect`] through the owned
//! [`Value`] tree. Numbers are parsed from raw lexemes and serialized unchanged,
//! including oversized integers, long decimals and exponents outside f64 range.
//! Missing precision is [`Precision::Observed`]; high precision is explicit.
//! TI-38 adds [`FieldPrecisions`] and [`inspect_fields`] for snapshot-driven
//! numeric leaves, plus repeated-overflow evidence in the standalone learner.
//!
//! TI-10 adds [`inference`]: bounded shape learning with optional structural
//! spec priors and immutable, versioned local baselines. It does not classify
//! drift or wire a decision into the guard.
//!
//! Inherited constraints, not renegotiable in this crate:
//! - the client's bytes are never re-encoded; inspection runs on a side channel
//!   (`TECH_STACK.md` §1.2);
//! - a number read from the wire is written back unchanged (contract §3.1);
//! - propagate with `?`; the workspace lints deny `unwrap`/`expect` in library
//!   code (`AGENTS.md` §3.1).

pub mod inference;
pub mod numeric;
pub mod parse;
pub mod precision;
pub mod tee;
pub mod value;

// Re-exported flat so callers never write a module path: `inspect` is the
// function, `parse` is the module behind it. Naming both `inspect` made rustdoc
// warn that the name was ambiguous, and it will spread into every call site.
pub use numeric::{Number, Precision};
pub use parse::{Inspection, Parse, ParseClass, ParseFailure, inspect, inspect_fields};
pub use precision::{FieldPrecisions, PrecisionError};
pub use tee::{DEFAULT_CAP, Tee};
pub use value::Value;
