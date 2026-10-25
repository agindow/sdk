//! TI-50 significance from raw detector windows; runtime contract §10.24.
//! Statistical independence and predeclared hypothesis families are caller
//! obligations. No I/O, guard activation, policy or repair decisions occur here.
//!
//! ```
//! use agw_stats::{control::Floors, significance};
//! let report = significance::unit("/amount", &[1.0; 100], &[100.0; 100], 1)?;
//! let floors = Floors::new(0.05, 0.2, 0.4, &[])?;
//! assert!(report.sufficient());
//! assert!(report.decision(0, &floors, "tenant")?.is_some_and(|d| d.eligible()));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
mod enumeration;
mod field;
mod math;
mod report;
mod unit;
pub use enumeration::enumeration;
pub use field::{charset, coercion, completeness};
pub use math::{fisher, magnitude};
pub use report::{Hypothesis, Report, Test};
pub use unit::unit;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    InvalidInput,
    ResourceLimit,
    Unit(crate::unit::Error),
    Enumeration(crate::enumeration::Error),
    Field(crate::field::Error),
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    FisherExact,
    /// Conservative single-sample DKW bounds plus a union/triangle bound.
    MagnitudeDkwBound,
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Statistic {
    pub method: Method,
    pub samples: (usize, usize),
    pub effect: f64,
    /// Unadjusted p-value; TI-19 applies family correction exactly once.
    pub p_value: f64,
}
