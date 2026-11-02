//! Checked decimal arithmetic; never converts a JSON number through floating point.
#[derive(Clone, Copy)]
pub(crate) struct Decimal {
    pub coefficient: i128,
    pub places: u32,
}
impl Decimal {
    pub fn parse(text: &str) -> Option<Self> {
        if text.len() > 80 || !matches!(text.as_bytes().first(), Some(b'0'..=b'9' | b'-')) {
            return None;
        }
        let raw: &serde_json::value::RawValue = serde_json::from_str(text).ok()?;
        if raw.get() != text {
            return None;
        }
        let (mantissa, exponent) = match text.find(['e', 'E']) {
            Some(i) => (&text[..i], text[i + 1..].parse::<i32>().ok()?),
            None => (text, 0),
        };
        if exponent.unsigned_abs() > 18 {
            return None;
        }
        let places = mantissa.find('.').map_or(0, |i| mantissa.len() - i - 1) as i32 - exponent;
        let digits = mantissa.replace('.', "");
        if digits.trim_start_matches('-').len() > 36 {
            return None;
        }
        let mut coefficient = digits.parse::<i128>().ok()?;
        let places = if places < 0 {
            coefficient = coefficient.checked_mul(10_i128.checked_pow((-places) as u32)?)?;
            0
        } else {
            places as u32
        };
        Self::normalize(coefficient, places)
    }
    fn normalize(mut coefficient: i128, mut places: u32) -> Option<Self> {
        while places > 0 && coefficient % 10 == 0 {
            coefficient /= 10;
            places -= 1;
        }
        if places > 18 || coefficient.unsigned_abs().to_string().len() > 36 {
            return None;
        }
        Some(Self {
            coefficient,
            places,
        })
    }
    pub fn scale(self, numerator: i128, denominator: i128) -> Option<Self> {
        let mut n = self.coefficient.checked_mul(numerator)?;
        if n == i128::MIN {
            return None;
        }
        let mut d = denominator;
        let divisor = gcd(n, d);
        n /= divisor;
        d /= divisor;
        let mut places = self.places;
        // A finite decimal denominator contains only factors 2 and 5.
        while d != 1 && places < 18 {
            let common = gcd(10, d);
            if common == 1 {
                return None;
            }
            n = n.checked_mul(10 / common)?;
            d /= common;
            places += 1;
        }
        if d != 1 {
            return None;
        }
        Self::normalize(n, places)
    }
    pub fn compare(self, other: Self) -> Option<std::cmp::Ordering> {
        let places = self.places.max(other.places);
        Some(
            self.coefficient
                .checked_mul(10_i128.checked_pow(places - self.places)?)?
                .cmp(
                    &other
                        .coefficient
                        .checked_mul(10_i128.checked_pow(places - other.places)?)?,
                ),
        )
    }
    pub fn render(self) -> String {
        let sign = if self.coefficient < 0 { "-" } else { "" };
        let mut digits = self.coefficient.unsigned_abs().to_string();
        if self.places == 0 {
            return format!("{sign}{digits}");
        }
        while digits.len() <= self.places as usize {
            digits.insert(0, '0');
        }
        digits.insert(digits.len() - self.places as usize, '.');
        format!("{sign}{digits}")
    }
}
fn gcd(mut a: i128, mut b: i128) -> i128 {
    a = a.abs();
    while b != 0 {
        let remainder = a % b;
        a = b;
        b = remainder;
    }
    a.max(1)
}
