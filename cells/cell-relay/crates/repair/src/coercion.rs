//! The repair path's one forbidden proposal — TI-07, contract §3.1.
//!
//! **A repair may change a numeric value. It may not coerce a numeric type.**
//!
//! TI-66's explicit empty-string-to-null exception is nonnumeric and is guarded
//! by planning and raw-source staging; this numeric fence remains unchanged.
//!
//! That numeric fence protects identifiers. Changing `9007199254740993` cents to dollars
//! is the product working. Changing `"a_1B2C3D"` to `1234567` is a different ID
//! wearing the same field name, and no confidence score makes it safe — the
//! coercion is destructive *before* any judgement is applied to it. So the
//! proposal is refused at the boundary, structurally, rather than scored and
//! gated downstream.
//!
//! ## Why a refusal, not a repair
//!
//! Contract §3.1 requirement 3 says it outright: *"Never coerce numerics. If a
//! field must change type, string→number on an ID is a corruption, not a repair.
//! Reject and classify as `semantic`."* Classification is not left to the caller
//! — [`Refusal`] carries [`DriftClass::Semantic`] — because a refusal the caller
//! has to classify is a refusal that eventually gets classified as `shape`, and
//! `shape` fails open (contract §4). A coercion that fails open is the worst
//! possible outcome: upstream returns 200, the agent writes to the wrong charge.
//!
//! ## What this rule deliberately does not cover
//!
//! Numeric value changes (`Number` → `Number`) are **permitted**, and that is not
//! an oversight — it is the fix for the dominant write-side repair. Unit flips,
//! epoch-versus-milliseconds and rounding fixes are all number-to-number, and
//! forbidding them would forbid selective repair outright.
//!
//! What keeps *those* honest is not this module but
//! [`agw_inspector::numeric::Number`]: a number read off the wire is written back
//! as its source lexeme, so a repair that leaves a value alone cannot perturb it.
//! Corruption has to be proposed to happen.
//!
//! [`agw_inspector::numeric::Number`]: ../../inspector/src/numeric.rs

use std::fmt;

use agw_diff::DriftClass;

/// The JSON type of a value, as far as this rule cares.
///
/// Coarser than the value it describes on purpose: the question is never "is this
/// the same number" but "is this the same *kind* of thing".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JsonType {
    Null,
    Bool,
    Number,
    String,
    Array,
    Object,
}

/// One proposed repair: `path` would go from `from` to `to`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Proposal<'a> {
    /// Dot-notation path, as TI-10's inference reports it (`charge.amount`).
    pub path: &'a str,
    pub from: JsonType,
    pub to: JsonType,
}

/// Why a proposal was refused. Carries everything the audit artifact needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Refusal<'a> {
    pub path: &'a str,
    pub from: JsonType,
    pub to: JsonType,
    /// Always [`DriftClass::Semantic`]. See the module docs for why the caller
    /// does not get to choose.
    pub class: DriftClass,
}

/// Admit a repair proposal, or refuse it.
///
/// Total and infallible: a refusal is a value the caller classifies and records,
/// never an error it has to handle (contract §4.4's shape, applied to repairs).
///
/// The refusal borrows the path out of the proposal rather than copying it. The
/// lifetime comes from the `str`, not from the `&Proposal`, so a proposal built
/// inline (`check(&Proposal { path, from, to })`) is fine.
pub fn check<'a>(proposal: &Proposal<'a>) -> Result<(), Refusal<'a>> {
    let Proposal { path, from, to } = *proposal;

    // A type change that involves a number is the corruption. Both directions:
    // neither a number nor a string can be reconstructed faithfully as the other.
    if from != to && (from == JsonType::Number || to == JsonType::Number) {
        return Err(Refusal {
            path,
            from,
            to,
            class: DriftClass::Semantic,
        });
    }
    Ok(())
}

impl fmt::Display for Refusal<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "refused to coerce `{}` from {} to {} (contract §3.1: a type change on \
             a number is a corruption, not a repair)",
            self.path, self.from, self.to
        )
    }
}

impl fmt::Display for JsonType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Null => "null",
            Self::Bool => "bool",
            Self::Number => "number",
            Self::String => "string",
            Self::Array => "array",
            Self::Object => "object",
        };
        f.write_str(name)
    }
}
