//! TI-52 exact decimal/binary error proofs and explicit local unit projection.
//! No formatted-float oracle, payload edits, callbacks or implicit guard upgrade.
mod config;
mod math;
mod window;
use agw_inspector::Number;
pub use config::{Budget, Hint, Limits, Mode, Policy, PrecisionSource};
use math::{Rational, State};
pub use window::{Comparison, NumericSample, UnitWindow, compare};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    InvalidInput,
    ResourceLimit,
    Protected,
    Unrepresentable,
    Overflow,
    Underflow,
    UnsafeInteger,
    Inexact,
    BudgetExceeded,
    Unauthorized,
    IncompatiblePolicies,
    NoNumeric,
    Detector(crate::unit::Error),
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}
/// Exact nonnegative rational, deliberately not promised to be reduced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fraction {
    numerator: String,
    denominator: String,
}
impl Fraction {
    pub fn numerator(&self) -> &str {
        &self.numerator
    }
    pub fn denominator(&self) -> &str {
        &self.denominator
    }
    fn new(r: Rational, state: &mut State) -> Result<Self, Error> {
        // log10(2) < 1/3, so this integer bound precharges decimal conversion.
        let (n, d) = (r.n.bits().max(1) as usize, r.d.bits().max(1) as usize);
        state.work(
            n.checked_mul(n)
                .and_then(|c| d.checked_mul(d).and_then(|b| c.checked_add(b)))
                .ok_or(Error::ResourceLimit)?,
        )?;
        state.output((r.n.bits() / 3 + r.d.bits() / 3 + 2) as usize)?;
        Ok(Self {
            numerator: r.n.to_str_radix(10),
            denominator: r.d.to_str_radix(10),
        })
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct Observation {
    authorization: Policy,
    source: String,
    value: f64,
    absolute_error: Fraction,
    relative_error: Option<Fraction>,
    exact: bool,
}
impl Observation {
    pub fn authorization(&self) -> Policy {
        self.authorization
    }
    pub fn source(&self) -> &str {
        &self.source
    }
    pub fn value(&self) -> f64 {
        self.value
    }
    pub fn absolute_error(&self) -> &Fraction {
        &self.absolute_error
    }
    pub fn relative_error(&self) -> Option<&Fraction> {
        self.relative_error.as_ref()
    }
    pub fn exact(&self) -> bool {
        self.exact
    }
}
/// Standalone projection with the same per-call bounds as window materialization.
pub fn project(number: &Number, policy: Policy) -> Result<Observation, Error> {
    checked(number, policy, &mut State::new(policy.limits()))
}
fn checked(number: &Number, policy: Policy, state: &mut State) -> Result<Observation, Error> {
    if policy.precision().hint == Hint::HighPrecision {
        return Err(Error::Protected);
    }
    if matches!(number, Number::Unrepresentable(_)) {
        return Err(Error::Unrepresentable);
    }
    let source = number.lexeme();
    let decimal = math::decimal(&source, policy.limits(), state)?;
    // Public lossless trees can be caller-built. A retained plain integer must
    // not bypass Observed's representability fence by choosing another variant.
    if !source.contains(['.', 'e', 'E'])
        && source.parse::<i64>().is_err()
        && source.parse::<u64>().is_err()
    {
        return Err(Error::Unrepresentable);
    }
    if !policy.allows_unsafe_integers() && decimal.unsafe_integer(state)? {
        return Err(Error::UnsafeInteger);
    }
    let value = source.parse::<f64>().map_err(|_| Error::InvalidInput)?;
    if !value.is_finite() {
        return Err(Error::Overflow);
    }
    if value == 0. && decimal.n.bits() != 0 {
        return Err(Error::Underflow);
    }
    let binary = Rational::binary(value, state)?;
    let error = decimal.difference(&binary, state)?;
    let exact = error.n.bits() == 0;
    match policy.mode() {
        Mode::ExactOnly if !exact => return Err(Error::Inexact),
        Mode::Approximate(budget) if !exact => {
            let absolute = Rational::binary(budget.absolute(), state)?;
            if !error.le(&absolute, state)? {
                let relative =
                    Rational::binary(budget.relative(), state)?.times(&decimal, state)?;
                if !error.le(&relative, state)? {
                    return Err(Error::BudgetExceeded);
                }
            }
        }
        _ => {}
    }
    let relative = error.relative(&decimal, state)?;
    state.output(std::mem::size_of::<Observation>() + source.len())?;
    Ok(Observation {
        authorization: policy,
        source: source.into_owned(),
        value,
        relative_error: relative.map(|r| Fraction::new(r, state)).transpose()?,
        absolute_error: Fraction::new(error, state)?,
        exact,
    })
}
