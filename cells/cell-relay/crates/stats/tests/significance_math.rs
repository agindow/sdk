use agw_stats::significance::{Error, Method, fisher, magnitude};

#[test]
fn fisher_matches_scipy_reference_tables_and_symmetries() -> Result<(), Error> {
    // SciPy's probability-ordered two-sided examples, not doubled one-tail tests.
    for (a, na, b, nb, expected) in [
        (6, 8, 1, 5, 0.10256410256410257),
        (8, 10, 1, 6, 0.034965034965034975),
        (3, 4, 1, 4, 0.4857142857142857),
    ] {
        let test = fisher(a, na, b, nb)?;
        assert!((test.p_value - expected).abs() < 2e-12);
        assert_eq!(test.method, Method::FisherExact);
        assert_eq!(test.samples, (na, nb));
        assert!((test.effect - (a as f64 / na as f64 - b as f64 / nb as f64).abs()).abs() < 1e-15);
        assert!((test.p_value - fisher(b, nb, a, na)?.p_value).abs() < 2e-12);
        assert!((test.p_value - fisher(na - a, na, nb - b, nb)?.p_value).abs() < 2e-12);
    }
    Ok(())
}
#[test]
fn equal_degenerate_rates_are_one_and_extreme_tails_are_positive() -> Result<(), Error> {
    assert_eq!(fisher(0, 20, 0, 30)?.p_value, 1.0);
    assert_eq!(fisher(20, 20, 30, 30)?.p_value, 1.0);
    assert_eq!(fisher(10, 20, 15, 30)?.p_value, 1.0);
    let tail = fisher(0, 4096, 4096, 4096)?.p_value;
    assert!(tail > 0.0 && tail < 1e-9);
    Ok(())
}
#[test]
fn numeric_bound_handles_ties_unequal_samples_and_extreme_magnitudes() -> Result<(), Error> {
    let same = magnitude(&[1.0, -1.0], &[1.0; 4])?;
    assert_eq!(same.effect, 0.0);
    assert_eq!(same.p_value, 1.0);
    let a = [0.0; 20];
    let b = [f64::MAX; 80];
    let test = magnitude(&a, &b)?;
    assert_eq!(test.effect, 1.0);
    assert_eq!(test.method, Method::MagnitudeDkwBound);
    // r=20*80/(sqrt(20)+sqrt(80))² = 80/9.
    let expected = 4.0 * (-2.0_f64 * (80.0 / 9.0) * (1.0_f64 - 1e-12).powi(2)).exp() + 1e-12;
    assert!((test.p_value - expected).abs() < 1e-15);
    assert_eq!(test.p_value, magnitude(&b, &a)?.p_value);
    assert_eq!(
        magnitude(&[1.0, 1.0, 2.0], &[1.0, 2.0, 2.0])?.effect,
        1.0 / 3.0
    );
    Ok(())
}
#[test]
fn validates_counts_samples_and_non_finite_numbers_before_allocating() {
    for args in [(2, 1, 0, 1), (0, 0, 0, 1), (0, 1, 2, 1)] {
        assert_eq!(
            fisher(args.0, args.1, args.2, args.3),
            Err(Error::InvalidInput)
        );
    }
    assert_eq!(fisher(0, 4097, 0, 1), Err(Error::ResourceLimit));
    assert_eq!(magnitude(&[], &[1.0]), Err(Error::InvalidInput));
    assert_eq!(magnitude(&[f64::NAN], &[1.0]), Err(Error::InvalidInput));
    assert_eq!(
        magnitude(&[1.0], &[f64::INFINITY]),
        Err(Error::InvalidInput)
    );
    assert_eq!(
        magnitude(&vec![0.0; 4097], &[1.0]),
        Err(Error::ResourceLimit)
    );
}
#[test]
fn fisher_matches_independent_integer_combinatorics_for_small_tables() -> Result<(), Error> {
    let choose = |n: usize, k: usize| -> u64 {
        (0..k).fold(1, |value, i| value * (n - i) as u64 / (i + 1) as u64)
    };
    for n in 1..=6 {
        for m in 1..=6 {
            for a in 0..=n {
                for b in 0..=m {
                    let successes = a + b;
                    let observed = choose(n, a) * choose(m, b);
                    let tail: u64 = (successes.saturating_sub(m)..=n.min(successes))
                        .map(|x| choose(n, x) * choose(m, successes - x))
                        .filter(|mass| *mass <= observed)
                        .sum();
                    let expected = tail as f64 / choose(n + m, successes) as f64;
                    assert!(
                        (fisher(a, n, b, m)?.p_value - expected).abs() < 2e-12,
                        "table successes {a}/{n}, {b}/{m}"
                    );
                }
            }
        }
    }
    Ok(())
}
