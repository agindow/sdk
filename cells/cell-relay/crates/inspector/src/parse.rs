//! Structured inspection of a captured head — TI-06, contract §4.4.
//!
//! The module is `parse` and the public entry point is the function
//! [`inspect`]: two different names for two different things, so no call site
//! has to disambiguate which `inspect` it means.
//!
//! The output is a classification input, not a verdict: `agw-diff` decides the
//! drift class and kind. Nothing here can fail the client path — a parse
//! failure is a value.
//!
//! ```
//! use agw_inspector::{Number, Parse, Value, inspect};
//!
//! let outcome = inspect(b"18446744073709551616", false, None);
//! assert_eq!(outcome.parse, Parse::Parsed(Value::Number(
//!     Number::Unrepresentable("18446744073709551616".into())
//! )));
//! ```

use crate::{FieldPrecisions, Precision, Value, precision::Policy};
use serde_json::error::Category;

/// What structured inspection found in the retained head.
#[derive(Debug, Clone, PartialEq)]
pub struct Inspection {
    /// The parsed head, or why it could not be parsed.
    pub parse: Parse,
    /// True when the capture is a *prefix of an unfinished body* — cut by the
    /// tee's cap, or by an upstream failure part-way through. Independent of
    /// `parse`: a truncated capture that fails with [`ParseClass::Eof`] was cut
    /// short, not malformed, and a classifier must not report it as
    /// `malformed_body`. The two are also not exclusive — a head can parse and
    /// still be a prefix.
    pub truncated: bool,
}

/// The parse outcome of a captured head.
#[derive(Debug, Clone, PartialEq)]
pub enum Parse {
    Parsed(Value),
    /// A classification input (contract §4.4) — never a client-visible error.
    Unparseable(ParseFailure),
}

/// Why a capture could not be parsed, in the vocabulary the classifier reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseFailure {
    pub class: ParseClass,
    /// 1-based position where parsing stopped; 0 when the error carries no
    /// position (I/O failures).
    pub line: usize,
    pub column: usize,
    pub message: String,
}

/// The parser's error categories, renamed so classification does not depend on
/// which JSON parser ran.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseClass {
    /// Input ended before a value was complete — expected when truncated.
    Eof,
    /// Not syntactically valid JSON.
    Syntax,
    /// Valid JSON, wrong shape for what the caller asked for.
    Data,
    /// The underlying reader failed.
    Io,
}

/// Parse a captured head. Total: every outcome is a value (contract §4.4).
/// `None` uses [`Precision::Observed`]; callers must explicitly opt into
/// [`Precision::HighPrecision`]. This decision applies to the whole document.
pub fn inspect(captured: &[u8], truncated: bool, precision: Option<Precision>) -> Inspection {
    inspect_policy(
        captured,
        truncated,
        Policy::Whole(precision.unwrap_or_default()),
    )
}

/// Inspect using exact field paths compiled from the selected snapshot.
/// Incompleteness remains independent of parse success, as with [`inspect`].
pub fn inspect_fields(captured: &[u8], truncated: bool, fields: &FieldPrecisions) -> Inspection {
    inspect_policy(captured, truncated, Policy::Fields(fields))
}

fn inspect_policy(captured: &[u8], truncated: bool, precision: Policy<'_>) -> Inspection {
    let parse = match Value::from_slice(captured, precision) {
        Ok(value) => Parse::Parsed(value),
        Err(failure) => Parse::Unparseable(capture_failure(captured, failure)),
    };
    Inspection { parse, truncated }
}

// serde_json's raw-token scanner calls an unfinished numeric token Syntax,
// whereas its typed parser calls it Eof. Preserve the existing vocabulary
// without running a numeric value through that parser's f64 representation.
fn capture_failure(captured: &[u8], mut failure: ParseFailure) -> ParseFailure {
    if failure.class == ParseClass::Syntax
        && captured
            .last()
            .is_some_and(|b| matches!(b, b'.' | b'e' | b'E' | b'+' | b'-'))
        && failure.message.starts_with("invalid number")
    {
        let mut completed = captured.to_vec();
        completed.push(b'0');
        let completed = serde_json::from_slice::<&serde_json::value::RawValue>(&completed);
        if completed.is_ok() || completed.is_err_and(|e| e.classify() == Category::Eof) {
            failure.class = ParseClass::Eof;
            failure.message = format!(
                "EOF while parsing a value at line {} column {}",
                failure.line, failure.column
            );
        }
    }
    failure
}

impl From<&serde_json::Error> for ParseFailure {
    fn from(error: &serde_json::Error) -> Self {
        Self {
            class: match error.classify() {
                Category::Io => ParseClass::Io,
                Category::Syntax => ParseClass::Syntax,
                Category::Data => ParseClass::Data,
                Category::Eof => ParseClass::Eof,
            },
            line: error.line(),
            column: error.column(),
            message: error.to_string(),
        }
    }
}
