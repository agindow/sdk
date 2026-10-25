//! Drift classification — how AGINDOW names *what kind* of drift it found.
//!
//! Contract §3, response header `X-Agindow-Drift-Class`. These three strings are
//! a public interface: a customer's policy gate matches on them, so renaming one
//! is a breaking change and adding one is a contract revision.
//!
//! Split out from the shape diff that produces it so the vocabulary can be read,
//! pinned and consumed without pulling in the differ. The rule this crate is
//! otherwise built on — uncertainty resolves to [`DriftClass::Unknown`], never to
//! a guess — starts here, where the third variant exists at all.

use std::fmt;

/// The three drift classes AGINDOW reports.
///
/// Which class a finding gets is what selects the failure behaviour: shape drift
/// fails open (upstream will reject it), semantic drift fails closed (upstream
/// will return 200 with the wrong data). Contract §4.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DriftClass {
    /// Structure changed: a field was renamed, removed, or added as required.
    /// Upstream rejects it, so passing through costs a retry.
    Shape,
    /// Meaning changed under a stable shape: units flipped, a field swapped, an
    /// enum widened. Upstream returns 200 with wrong data.
    Semantic,
    /// AGINDOW cannot say. Never a guess — uncertainty is a first-class answer,
    /// and the policy gate decides what to do about it.
    Unknown,
}

impl DriftClass {
    /// The exact string that goes in `X-Agindow-Drift-Class`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Shape => "shape",
            Self::Semantic => "semantic",
            Self::Unknown => "unknown",
        }
    }

    /// Read a wire value back. `None` for anything the contract does not define,
    /// so an unrecognised value can never be mistaken for [`DriftClass::Unknown`].
    ///
    /// Not `const`: matching on `&str` needs `PartialEq`, which is not const-stable
    /// yet. [`Self::as_str`] is the half that benefits from being `const`.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "shape" => Some(Self::Shape),
            "semantic" => Some(Self::Semantic),
            "unknown" => Some(Self::Unknown),
            _ => None,
        }
    }
}

impl fmt::Display for DriftClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
