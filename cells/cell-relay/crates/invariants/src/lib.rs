//! TI-21: standalone typed invariant DSL, compiled scalar checks and local histories.
//!
//! Compilation, learning and publication are administrative. Evaluation only
//! reads borrowed slots; it does not parse JSON, allocate, fetch or apply policy.
pub mod activation;
pub mod cross_endpoint;
mod evaluator;
pub mod learning;
pub mod store;
pub use evaluator::{CompiledRule, Program, compile};

/// Hard administrative bounds also bound per-rule evaluation work.
pub const MAX_RULES: usize = 128;
pub const MAX_BINDINGS: usize = 128;
pub const MAX_TERMS: usize = 32;
pub const MAX_ENUM: usize = 128;
pub const MAX_TEXT: usize = 256;

/// Invalid definitions and capacity exhaustion never become runtime violations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Invalid,
    Capacity,
    NotFound,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}

/// Exact caller projection. Other numeric formats must remain unavailable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scalar<'a> {
    Integer(i64),
    Text(&'a str),
    Missing,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Pass,
    Violation,
    EnumAddition,
    Unavailable,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Relation {
    Equal,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
}

/// Compact typed DSL. Binding names may identify either endpoint; callers own
/// correlation and normalization. Tolerance is absolute, in the input unit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Form {
    Relation {
        left: String,
        relation: Relation,
        right: String,
    },
    Range {
        field: String,
        min: i64,
        max: i64,
    },
    Enum {
        field: String,
        values: Vec<String>,
    },
    Ordering {
        earlier: String,
        later: String,
    },
    Sum {
        total: String,
        terms: Vec<String>,
        tolerance: u64,
    },
    CrossEndpoint {
        left: String,
        right: String,
        tolerance: u64,
    },
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    pub id: String,
    pub form: Form,
}

pub(crate) fn valid_text(value: &str) -> bool {
    !value.is_empty() && value.len() <= MAX_TEXT
}

/// Borrowed audit evidence, independent of the eventual policy decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Evidence<'a> {
    pub actual: Scalar<'a>,
    pub expected: Expected<'a>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Expected<'a> {
    Numeric {
        value: i128,
        relation: Relation,
        tolerance: u64,
    },
    Range {
        min: i128,
        max: i128,
    },
    Vocabulary(&'a [String]),
}
