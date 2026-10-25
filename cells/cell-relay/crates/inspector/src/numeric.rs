//! Numeric fidelity — TI-07, contract §3.1.
//!
//! The bug class this module exists to make impossible: a JSON parse and
//! re-serialize destroys large integers, so a "repaired" charge ID comes back as a
//! *different* ID. In JavaScript that happens in `JSON.parse`, before any repair
//! logic runs. Here it is a choice, and this module makes the safe choice the
//! only one a caller can make by accident.
//!
//! Three properties, in order of how much they matter:
//!
//! 1. **The round trip is exact.** [`Number::read`] followed by
//!    [`Number::lexeme`] reproduces the input text byte for byte — including
//!    `-0`, non-canonical digits, and integers wider than `u64`. Re-serialization
//!    is the *repair* path, and it is allowed to be non-byte-identical by
//!    contract; it is not allowed to change a number.
//! 2. **The fast path is exact, not merely fast.** A 64-bit integer is stored as
//!    [`i64`]/[`u64`] — comparable, cheap, and a plain value. The lexeme fallback
//!    is taken only when the canonical form is *not* byte-identical to the input,
//!    which is what keeps property 1 unconditional.
//! 3. **Precision is a decision, not an inference.** [`Precision`] is supplied by
//!    the caller from the contract. AGINDOW never infers high precision from
//!    magnitude: a number is large far more often than it is precise, and the
//!    difference is the difference between an ID and a quantity.
//!
//! ## Why not `serde_json`'s `arbitrary_precision`
//!
//! That cargo feature is the obvious answer and it is the wrong one. It is a
//! crate-wide switch, so enabling it would give *every* crate lexeme-only numbers
//! and destroy the `i64`/`u64` fast path — and it would apply that cost to numbers
//! no contract ever flagged. Per-field behaviour has to be ours.
//!
//! [`contract_snapshot`] v2 supplies `high_precision` per field (TI-38).
//! [`crate::inspect_fields`] applies compiled hints; [`crate::inspect`] retains
//! the explicit whole-document API. Missing/false flags select Observed.
//!
//! [`contract_snapshot`]: ../../../../../contracts/contract_snapshot.v2.json

use std::borrow::Cow;
use std::fmt;

use serde::Serialize;
use serde::ser::Serializer;
use serde_json::value::RawValue;

/// How much precision AGINDOW claims over a number.
///
/// A missing flag is [`Precision::Observed`], never "assume high-precision":
/// absent data is not a licence to be lossier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Precision {
    /// The contract flags the field high-precision. Its numbers may exceed 64
    /// bits, so they are kept verbatim.
    HighPrecision,
    /// Unflagged. An integer outside the 64-bit range is retained but *not*
    /// modelled — see [`Number::Unrepresentable`]. The default.
    #[default]
    Observed,
}

/// A JSON number AGINDOW can write back without changing a digit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Number {
    /// Fast path: an integer inside the signed 64-bit range.
    I64(i64),
    /// Integers above `i64::MAX` — nanosecond epochs, `u64` primary keys.
    U64(u64),
    /// The source lexeme, verbatim: a decimal, an exponent, a negative zero, or
    /// an integer wider than 64 bits at a field flagged high-precision. This is
    /// the only lossless representation such a number has.
    Verbatim(Box<str>),
    /// An integer wider than 64 bits at a field the contract does *not* flag.
    ///
    /// The lexeme is retained so the classifier can still see it and the writer
    /// can still reproduce it, but AGINDOW declines to model it: an unflagged
    /// field carrying a 39-digit integer is itself drift signal, and pretending
    /// otherwise would be the confident-wrong failure this product exists to
    /// avoid. Never an error — a classification input (contract §4.4).
    Unrepresentable(Box<str>),
}

impl Number {
    /// Read a JSON number token. Total: every lexeme is a value, never an error.
    ///
    /// The fast path is taken only when re-serializing is provably byte-identical
    /// to the input. That single check is what makes [`Number::lexeme`] exact for
    /// *every* input rather than for every input except the awkward ones.
    pub fn read(lexeme: &str, precision: Precision) -> Self {
        let Some(digits) = integer_digits(lexeme) else {
            // A decimal, an exponent, or `-0`. No integer conversion is involved,
            // so there is nothing to truncate and the precision flag does not
            // apply — refusing `0.1` would be absurd, not cautious.
            return Self::Verbatim(lexeme.into());
        };

        if let Some(value) = digits
            .parse::<i64>()
            .ok()
            .filter(|v| v.to_string() == lexeme)
        {
            return Self::I64(value);
        }
        if let Some(value) = digits
            .parse::<u64>()
            .ok()
            .filter(|v| v.to_string() == lexeme)
        {
            return Self::U64(value);
        }

        // An integer with no exact home left. Either the contract says this field
        // is high-precision and we keep it, or it does not and we decline.
        match precision {
            Precision::HighPrecision => Self::Verbatim(lexeme.into()),
            Precision::Observed => Self::Unrepresentable(lexeme.into()),
        }
    }

    /// The number exactly as it must be written back.
    pub fn lexeme(&self) -> Cow<'_, str> {
        match self {
            Self::I64(value) => Cow::Owned(value.to_string()),
            Self::U64(value) => Cow::Owned(value.to_string()),
            Self::Verbatim(lexeme) | Self::Unrepresentable(lexeme) => Cow::Borrowed(lexeme),
        }
    }

    /// Length of [`Number::lexeme`] without materializing it.
    ///
    /// Budgeting and profiling classify numbers far more often than they need
    /// the text, so the integer fast path counts digits instead of allocating a
    /// `String` per sample.
    pub fn lexeme_len(&self) -> usize {
        match self {
            Self::I64(value) if *value < 0 => 1 + decimal_digits(value.unsigned_abs()),
            Self::I64(value) => decimal_digits(*value as u64),
            Self::U64(value) => decimal_digits(*value),
            Self::Verbatim(lexeme) | Self::Unrepresentable(lexeme) => lexeme.len(),
        }
    }

    /// The stored signed integer, only for [`Number::I64`].
    ///
    /// Variant accessor: does not convert `U64` or parse retained lexemes.
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Self::I64(value) => Some(*value),
            _ => None,
        }
    }

    /// The stored unsigned integer, only for [`Number::U64`].
    ///
    /// Variant accessor: even a non-negative `I64` returns `None`.
    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Self::U64(value) => Some(*value),
            _ => None,
        }
    }

    /// True when AGINDOW models this number faithfully.
    ///
    /// False only for [`Number::Unrepresentable`]. Every other variant round-trips
    /// exactly; this one round-trips too, it just declines to be understood.
    pub fn is_exact(&self) -> bool {
        !matches!(self, Self::Unrepresentable(_))
    }
}

impl fmt::Display for Number {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.lexeme().as_ref())
    }
}

/// Serializes to the exact source lexeme.
///
/// This is the point of the type: the repair path re-serializes, and contract
/// §3.1 allows that to stop being byte-identical but never allows it to change a
/// number. Emitting the lexeme directly makes that structural rather than a
/// convention every future call site has to remember.
impl Serialize for Number {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let raw =
            RawValue::from_string(self.lexeme().into_owned()).map_err(serde::ser::Error::custom)?;
        raw.serialize(serializer)
    }
}

/// Decimal digits in a canonical unsigned integer; zero has one.
fn decimal_digits(value: u64) -> usize {
    if value == 0 {
        1
    } else {
        value.ilog10() as usize + 1
    }
}

/// The lexeme when it is an integer worth trying to fit, `None` otherwise.
///
/// `None` covers decimals, exponents, and `-0`. That last one is deliberate:
/// JSON has exactly one zero, and `-0` is the float negative zero in every
/// language AGINDOW will meet. Parsing it as an integer would round-trip it to
/// `0` — precisely the reserializer bug this module prevents.
fn integer_digits(lexeme: &str) -> Option<&str> {
    if lexeme.is_empty() || lexeme.contains(['.', 'e', 'E']) {
        return None;
    }
    match lexeme.strip_prefix('-') {
        Some(digits) if digits.bytes().all(|byte| byte == b'0') && !digits.is_empty() => None,
        _ => Some(lexeme),
    }
}
