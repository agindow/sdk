use super::Error;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hint {
    Absent,
    Observed,
    HighPrecision,
}
/// Caller provenance from the selected immutable contract, not inferred from a
/// numeric variant. Absent and explicit false remain distinct identities.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrecisionSource {
    pub revision: u64,
    pub hint: Hint,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Budget {
    absolute: u64,
    relative: u64,
}
impl Budget {
    pub fn new(absolute: f64, relative: f64) -> Result<Self, Error> {
        if !absolute.is_finite() || !relative.is_finite() || absolute < 0. || relative < 0. {
            return Err(Error::InvalidInput);
        }
        Ok(Self {
            absolute: if absolute == 0. {
                0
            } else {
                absolute.to_bits()
            },
            relative: if relative == 0. {
                0
            } else {
                relative.to_bits()
            },
        })
    }
    pub fn absolute(self) -> f64 {
        f64::from_bits(self.absolute)
    }
    pub fn relative(self) -> f64 {
        f64::from_bits(self.relative)
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    ExactOnly,
    Approximate(Budget),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub token_bytes: usize,
    pub exponent: u32,
    pub work: usize,
    pub bytes: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            token_bytes: 4096,
            exponent: 1024,
            work: 100_000_000,
            bytes: 1024 * 1024,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Policy {
    mode: Mode,
    precision: PrecisionSource,
    unsafe_integers: bool,
    limits: Limits,
}
impl Policy {
    pub fn new(
        mode: Mode,
        precision: PrecisionSource,
        unsafe_integers: bool,
        limits: Limits,
    ) -> Result<Self, Error> {
        if precision.revision == 0
            || !(1..=4096).contains(&limits.token_bytes)
            || limits.exponent > 1024
            || !(1..=1_000_000_000).contains(&limits.work)
            || !(1..=16 * 1024 * 1024).contains(&limits.bytes)
        {
            return Err(Error::InvalidInput);
        }
        Ok(Self {
            mode,
            precision,
            unsafe_integers,
            limits,
        })
    }
    pub fn mode(self) -> Mode {
        self.mode
    }
    pub fn precision(self) -> PrecisionSource {
        self.precision
    }
    pub fn allows_unsafe_integers(self) -> bool {
        self.unsafe_integers
    }
    pub fn limits(self) -> Limits {
        self.limits
    }
}
