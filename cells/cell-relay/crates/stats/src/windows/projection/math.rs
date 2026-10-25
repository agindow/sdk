use super::{Error, Limits};
use num_bigint::BigUint;
use std::cmp::Ordering;

pub(super) struct State {
    work: usize,
    bytes: usize,
}
impl State {
    pub fn new(limits: Limits) -> Self {
        Self {
            work: limits.work,
            bytes: limits.bytes,
        }
    }
    pub fn work(&mut self, cost: usize) -> Result<(), Error> {
        self.work = self.work.checked_sub(cost).ok_or(Error::ResourceLimit)?;
        Ok(())
    }
    pub fn output(&mut self, bytes: usize) -> Result<(), Error> {
        self.bytes = self.bytes.checked_sub(bytes).ok_or(Error::ResourceLimit)?;
        Ok(())
    }
    pub fn mul(&mut self, a: &BigUint, b: &BigUint) -> Result<BigUint, Error> {
        self.product_cost(a, b)?;
        Ok(a * b)
    }
    pub fn product_cost(&mut self, a: &BigUint, b: &BigUint) -> Result<(), Error> {
        self.work(
            (a.bits().max(1) as usize)
                .checked_mul(b.bits().max(1) as usize)
                .ok_or(Error::ResourceLimit)?,
        )
    }
    pub fn cmp(&mut self, a: &BigUint, b: &BigUint) -> Result<Ordering, Error> {
        self.work((a.bits() + b.bits()) as usize)?;
        Ok(a.cmp(b))
    }
    pub fn pow10(&mut self, exponent: u32) -> Result<BigUint, Error> {
        // Powers are bounded before this call. Quadratic bit charging covers
        // the finite multiplication series used by exponentiation by squaring.
        let bits = exponent as usize * 4 + 1;
        self.work(bits.checked_mul(bits).ok_or(Error::ResourceLimit)?)?;
        Ok(BigUint::from(10u8).pow(exponent))
    }
}
#[derive(Debug)]
pub(super) struct Rational {
    pub n: BigUint,
    pub d: BigUint,
}
impl Rational {
    pub fn zero() -> Self {
        Self {
            n: BigUint::from(0u8),
            d: BigUint::from(1u8),
        }
    }
    pub fn binary(value: f64, state: &mut State) -> Result<Self, Error> {
        let bits = value.to_bits() & !(1u64 << 63);
        let e = ((bits >> 52) & 2047) as i32;
        let mut significand = bits & ((1u64 << 52) - 1);
        let mut exponent = if e == 0 { -1074 } else { e - 1023 - 52 };
        if e != 0 {
            significand |= 1u64 << 52;
        }
        if significand == 0 {
            return Ok(Self::zero());
        }
        let zeros = significand.trailing_zeros();
        significand >>= zeros;
        exponent += zeros as i32;
        state.work(64 + exponent.unsigned_abs() as usize)?;
        let n = BigUint::from(significand);
        Ok(if exponent >= 0 {
            Self {
                n: n << exponent as usize,
                d: BigUint::from(1u8),
            }
        } else {
            Self {
                n,
                d: BigUint::from(1u8) << exponent.unsigned_abs() as usize,
            }
        })
    }
    pub fn difference(&self, other: &Self, state: &mut State) -> Result<Self, Error> {
        let a = state.mul(&self.n, &other.d)?;
        let b = state.mul(&other.n, &self.d)?;
        let order = state.cmp(&a, &b)?;
        if order == Ordering::Equal {
            return Ok(Self::zero());
        }
        state.work(a.bits().max(b.bits()) as usize)?;
        let n = if order == Ordering::Greater {
            a - b
        } else {
            b - a
        };
        Ok(Self {
            n,
            d: state.mul(&self.d, &other.d)?,
        })
    }
    pub fn le(&self, other: &Self, state: &mut State) -> Result<bool, Error> {
        let a = state.mul(&self.n, &other.d)?;
        let b = state.mul(&other.n, &self.d)?;
        Ok(state.cmp(&a, &b)? != Ordering::Greater)
    }
    pub fn times(&self, other: &Self, state: &mut State) -> Result<Self, Error> {
        Ok(Self {
            n: state.mul(&self.n, &other.n)?,
            d: state.mul(&self.d, &other.d)?,
        })
    }
    pub fn relative(&self, original: &Self, state: &mut State) -> Result<Option<Self>, Error> {
        if original.n.bits() == 0 {
            return Ok(None);
        }
        if self.n.bits() == 0 {
            return Ok(Some(Self::zero()));
        }
        Ok(Some(Self {
            n: state.mul(&self.n, &original.d)?,
            d: state.mul(&self.d, &original.n)?,
        }))
    }
    pub fn unsafe_integer(&self, state: &mut State) -> Result<bool, Error> {
        state.product_cost(&self.n, &self.d)?;
        if &self.n % &self.d != BigUint::from(0u8) {
            return Ok(false);
        }
        let boundary = state.mul(&self.d, &BigUint::from(9_007_199_254_740_991u64))?;
        Ok(state.cmp(&self.n, &boundary)? == Ordering::Greater)
    }
}
pub(super) fn decimal(source: &str, limits: Limits, state: &mut State) -> Result<Rational, Error> {
    if source.len() > limits.token_bytes {
        return Err(Error::ResourceLimit);
    }
    state.work(source.len())?;
    let raw: &serde_json::value::RawValue =
        serde_json::from_str(source).map_err(|_| Error::InvalidInput)?;
    if raw.get() != source || !matches!(source.as_bytes().first(), Some(b'-' | b'0'..=b'9')) {
        return Err(Error::InvalidInput);
    }
    let unsigned = source.strip_prefix('-').unwrap_or(source);
    let (mantissa, exponent) = match unsigned.split_once(['e', 'E']) {
        Some((m, e)) => (m, e.parse::<i32>().map_err(|_| Error::ResourceLimit)?),
        None => (unsigned, 0),
    };
    let fractional = mantissa.split_once('.').map_or(0, |(_, f)| f.len());
    let digits = mantissa.len() - usize::from(mantissa.contains('.'));
    if digits > 1024 || exponent.unsigned_abs() > limits.exponent {
        return Err(Error::ResourceLimit);
    }
    let exponent = exponent
        .checked_sub(fractional as i32)
        .ok_or(Error::ResourceLimit)?;
    if exponent.unsigned_abs() > limits.exponent {
        return Err(Error::ResourceLimit);
    }
    state.work(
        digits
            .checked_mul(digits)
            .and_then(|c| c.checked_mul(16))
            .ok_or(Error::ResourceLimit)?,
    )?;
    let digits: Vec<_> = mantissa.bytes().filter(|b| *b != b'.').collect();
    let coefficient = BigUint::parse_bytes(&digits, 10).ok_or(Error::InvalidInput)?;
    if coefficient.bits() == 0 {
        return Ok(Rational::zero());
    }
    let power = state.pow10(exponent.unsigned_abs())?;
    Ok(if exponent >= 0 {
        Rational {
            n: state.mul(&coefficient, &power)?,
            d: BigUint::from(1u8),
        }
    } else {
        Rational {
            n: coefficient,
            d: power,
        }
    })
}
