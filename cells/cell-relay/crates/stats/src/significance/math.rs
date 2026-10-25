use super::{Error, Method, Statistic};

const ROUNDING: f64 = 1e-12;
fn lengths(n: usize, m: usize) -> Result<(), Error> {
    if n > 4096 || m > 4096 {
        return Err(Error::ResourceLimit);
    }
    if n == 0 || m == 0 {
        return Err(Error::InvalidInput);
    }
    Ok(())
}
/// Probability-ordered, two-sided Fisher exact test for successes a/b in
/// independent windows of sizes n/m. Enumerates the fixed-margin support.
/// See the SciPy specification linked in runtime contract §10.24.
pub fn fisher(a: usize, n: usize, b: usize, m: usize) -> Result<Statistic, Error> {
    lengths(n, m)?;
    if a > n || b > m {
        return Err(Error::InvalidInput);
    }
    let successes = a + b;
    let low = successes.saturating_sub(m);
    let high = n.min(successes);
    let mode = ((n + 1) * (successes + 1) / (n + m + 2)).clamp(low, high);
    // Recurrence outwards from a mode avoids factorial overflow, cancellation
    // of log-factorials and numerical underflow at the normalization center.
    let mut weights = vec![0.0; high - low + 1];
    weights[mode - low] = 1.0;
    for x in mode..high {
        weights[x + 1 - low] = weights[x - low] * ((n - x) * (successes - x)) as f64
            / ((x + 1) * (m + x + 1 - successes)) as f64;
    }
    for x in (low + 1..=mode).rev() {
        weights[x - 1 - low] = weights[x - low] * (x * (m + x - successes)) as f64
            / ((n - x + 1) * (successes - x + 1)) as f64;
    }
    let observed = weights[a - low];
    let total: f64 = weights.iter().sum();
    // Include numerically tied masses conservatively, then pad rounding error.
    let tail: f64 = weights
        .iter()
        .filter(|w| **w <= observed * (1.0 + 1e-10))
        .sum();
    Ok(Statistic {
        method: Method::FisherExact,
        samples: (n, m),
        effect: (a * m).abs_diff(b * n) as f64 / (n * m) as f64,
        p_value: (tail / total + ROUNDING).min(1.0),
    })
}
/// Distribution-free magnitude ECDF comparison, with a conservative p-value
/// bound. Does not use an asymptotic KS formula or interpret PSI as a p-value.
pub fn magnitude(baseline: &[f64], observed: &[f64]) -> Result<Statistic, Error> {
    let (n, m) = (baseline.len(), observed.len());
    lengths(n, m)?;
    if baseline.iter().chain(observed).any(|x| !x.is_finite()) {
        return Err(Error::InvalidInput);
    }
    let mut a: Vec<_> = baseline.iter().map(|x| x.abs()).collect();
    let mut b: Vec<_> = observed.iter().map(|x| x.abs()).collect();
    a.sort_by(f64::total_cmp);
    b.sort_by(f64::total_cmp);
    let (mut i, mut j, mut distance) = (0, 0, 0);
    while i < n || j < m {
        let value = a
            .get(i)
            .copied()
            .unwrap_or(f64::INFINITY)
            .min(b.get(j).copied().unwrap_or(f64::INFINITY));
        while i < n && a[i] <= value {
            i += 1;
        }
        while j < m && b[j] <= value {
            j += 1;
        }
        distance = distance.max((i * m).abs_diff(j * n));
    }
    let effect = distance as f64 / (n * m) as f64;
    let scale = ((n as f64).sqrt() + (m as f64).sqrt()).powi(2);
    let effective = (n * m) as f64 / scale;
    // Triangle inequality + two one-sample DKWM bounds with equal exponents.
    // The small subtraction keeps floating ECDF rounding conservative too.
    let d = (effect - ROUNDING).max(0.0);
    let p_value = (4.0 * (-2.0 * effective * d * d).exp() + ROUNDING).min(1.0);
    Ok(Statistic {
        method: Method::MagnitudeDkwBound,
        samples: (n, m),
        effect,
        p_value,
    })
}
