use super::Error;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Metrics {
    pub wasserstein: f64,
    pub normalized_wasserstein: f64,
    pub psi: f64,
}

pub(super) fn validate(a: &[f64], b: &[f64]) -> Result<(), Error> {
    if a.len() > 4096 || b.len() > 4096 {
        return Err(Error::ResourceLimit);
    }
    if a.iter().chain(b).any(|v| !v.is_finite()) {
        return Err(Error::InvalidInput);
    }
    Ok(())
}

/// Uniform empirical Wasserstein-1 on absolute magnitudes, and PSI over shared
/// baseline decile bins. Unlike `detect`, this accepts small nonempty windows
/// for metric inspection; it makes no drift decision.
pub fn measure(baseline: &[f64], observed: &[f64]) -> Result<Metrics, Error> {
    validate(baseline, observed)?;
    if baseline.is_empty() || observed.is_empty() {
        return Err(Error::InvalidInput);
    }
    let mut a: Vec<_> = baseline.iter().map(|v| v.abs()).collect();
    let mut b: Vec<_> = observed.iter().map(|v| v.abs()).collect();
    a.sort_by(f64::total_cmp);
    b.sort_by(f64::total_cmp);
    let scale = a[a.len() - 1].max(b[b.len() - 1]);
    let (wasserstein, normalized_wasserstein) = if scale == 0. {
        (0., 0.)
    } else {
        // Normalize before arithmetic: finite inputs near f64::MAX must not
        // overflow merely because a sum or signed subtraction would do so.
        let mean_a = a.iter().map(|x| (x / scale) / a.len() as f64).sum::<f64>();
        let mean_b = b.iter().map(|x| (x / scale) / b.len() as f64).sum::<f64>();
        let (mut i, mut j) = (0, 0);
        let (mut left_a, mut left_b) = (b.len(), a.len());
        let mut distance = 0.;
        while i < a.len() && j < b.len() {
            // Integer masses avoid accumulating floating CDF boundary errors.
            let mass = left_a.min(left_b);
            distance += ((a[i] / scale) - (b[j] / scale)).abs()
                * (mass as f64 / (a.len() * b.len()) as f64);
            left_a -= mass;
            left_b -= mass;
            if left_a == 0 {
                i += 1;
                left_a = b.len();
            }
            if left_b == 0 {
                j += 1;
                left_b = a.len();
            }
        }
        let distance = distance.clamp(0., 1.);
        (distance * scale, distance / mean_a.max(mean_b))
    };
    let mut cuts: Vec<_> = (1..=10)
        .map(|k| a[(k * a.len()).div_ceil(10) - 1])
        .collect();
    cuts.dedup();
    let histogram = |values: &[f64]| {
        let mut counts = vec![0_usize; cuts.len() + 2];
        for x in values {
            let bin = if *x < a[0] {
                0
            } else {
                1 + cuts.partition_point(|cut| x > cut)
            };
            counts[bin] += 1;
        }
        let mut probabilities: Vec<_> = counts
            .iter()
            .map(|n| (*n as f64 / values.len() as f64).max(1e-6))
            .collect();
        let sum: f64 = probabilities.iter().sum();
        for p in &mut probabilities {
            *p /= sum;
        }
        probabilities
    };
    let psi = histogram(&a)
        .iter()
        .zip(histogram(&b))
        .map(|(p, q)| (p - q) * (p.ln() - q.ln()))
        .sum::<f64>()
        .max(0.);
    if ![wasserstein, normalized_wasserstein, psi]
        .iter()
        .all(|v| v.is_finite())
    {
        return Err(Error::NumericRange);
    }
    Ok(Metrics {
        wasserstein,
        normalized_wasserstein,
        psi,
    })
}
