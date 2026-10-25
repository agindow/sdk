use agw_inspector::{Number, Precision};
use agw_stats::windows::projection::{self, Budget, Hint, Limits, Mode, Policy, PrecisionSource};
use std::error::Error;
fn policy(mode: Mode, unsafe_integers: bool) -> Result<Policy, projection::Error> {
    Policy::new(
        mode,
        PrecisionSource {
            revision: 7,
            hint: Hint::Absent,
        },
        unsafe_integers,
        Limits::default(),
    )
}
fn project(token: &str, policy: Policy) -> Result<projection::Observation, projection::Error> {
    projection::project(&Number::read(token, Precision::Observed), policy)
}
fn fraction_is(
    f: &projection::Fraction,
    numerator: u128,
    denominator: u128,
) -> Result<(), Box<dyn Error>> {
    // Independent reference fractions: no f64 formatting or production checker.
    assert_eq!(
        f.numerator().parse::<u128>()? * denominator,
        numerator * f.denominator().parse::<u128>()?
    );
    Ok(())
}
#[test]
fn exact_decimal_exponent_and_signed_zero_tokens_survive() -> Result<(), Box<dyn Error>> {
    let policy = policy(Mode::ExactOnly, false)?;
    for (token, value) in [
        ("0.5", 0.5f64),
        ("-2.5", -2.5),
        ("1e3", 1000.),
        ("5.00e-1", 0.5),
        ("0.5000", 0.5),
        (
            "0.1000000000000000055511151231257827021181583404541015625",
            0.1,
        ),
        ("-0", -0.),
        ("-0.000e+20", -0.),
        ("0e-999", 0.),
    ] {
        let result = project(token, policy)?;
        assert_eq!(result.value().to_bits(), value.to_bits());
        assert_eq!(result.source(), token);
        assert!(result.exact());
        assert_eq!(result.authorization(), policy);
        fraction_is(result.absolute_error(), 0, 1)?;
        assert_eq!(result.relative_error().is_none(), value == 0.);
    }
    assert!(matches!(
        project("0.1", policy),
        Err(projection::Error::Inexact)
    ));
    Ok(())
}
#[test]
fn exact_binary_error_and_inclusive_absolute_relative_boundaries() -> Result<(), Box<dyn Error>> {
    let rounded = project(
        "0.1",
        policy(Mode::Approximate(Budget::new(0., 2f64.powi(-54))?), false)?,
    )?;
    assert!(!rounded.exact());
    assert_eq!(rounded.value(), 0.1);
    // 0.1 binary error = 1/(5*2^55), relative error = 2^-54.
    fraction_is(rounded.absolute_error(), 1, 180_143_985_094_819_840)?;
    fraction_is(
        rounded.relative_error().ok_or("relative")?,
        1,
        18_014_398_509_481_984,
    )?;
    assert!(
        project(
            "0.1",
            policy(
                Mode::Approximate(Budget::new(0., 2f64.powi(-54).next_down())?),
                false
            )?
        )
        .is_err()
    );
    // Exact midpoint between adjacent f64 values, specified as a decimal.
    let midpoint = "0.100000000000000012490009027033011079765856266021728515625";
    let boundary = 2f64.powi(-57);
    let result = project(
        midpoint,
        policy(Mode::Approximate(Budget::new(boundary, 0.)?), false)?,
    )?;
    assert_eq!(result.value().to_bits(), 0.1f64.to_bits());
    assert!(
        project(
            midpoint,
            policy(
                Mode::Approximate(Budget::new(boundary.next_down(), 0.)?),
                false
            )?
        )
        .is_err()
    );
    // Either bound may independently admit; there is no epsilon at zero.
    assert!(
        project(
            "0.1",
            policy(Mode::Approximate(Budget::new(1e-17, 0.)?), false)?
        )
        .is_ok()
    );
    assert!(
        project(
            "0.1",
            policy(Mode::Approximate(Budget::new(0., 0.)?), false)?
        )
        .is_err()
    );
    Ok(())
}
#[test]
fn protected_invalid_overflow_underflow_and_unsafe_integers_refuse() -> Result<(), Box<dyn Error>> {
    let generous = policy(Mode::Approximate(Budget::new(f64::MAX, 1.)?), false)?;
    for token in [
        "1e400",
        "1e-400",
        "18446744073709551616",
        "9007199254740992",
        "9007199254740992.0",
        "9.007199254740992e15",
    ] {
        assert!(project(token, generous).is_err(), "{token}");
    }
    for token in [
        "", "NaN", "Infinity", "01", " 0.5", "1.", "1e", "+1", "0.5 ",
    ] {
        assert!(
            projection::project(&Number::Verbatim(token.into()), generous).is_err(),
            "{token}"
        );
    }
    for hint in [Hint::HighPrecision, Hint::Observed] {
        let p = Policy::new(
            Mode::ExactOnly,
            PrecisionSource { revision: 7, hint },
            true,
            Limits::default(),
        )?;
        assert_eq!(
            projection::project(&Number::I64(1), p).is_ok(),
            hint == Hint::Observed
        );
    }
    assert_eq!(
        project("9007199254740991", policy(Mode::ExactOnly, false)?)?.value(),
        9_007_199_254_740_991.
    );
    assert_eq!(
        project("9007199254740992", policy(Mode::ExactOnly, true)?)?.value(),
        9_007_199_254_740_992.
    );
    assert!(project("9007199254740993", policy(Mode::ExactOnly, true)?).is_err());
    assert_eq!(
        project(
            "9007199254740993",
            policy(Mode::Approximate(Budget::new(1., 0.)?), true)?
        )?
        .value(),
        9_007_199_254_740_992.
    );
    Ok(())
}
#[test]
fn subnormals_require_the_same_proof_and_nonzero_never_rounds_to_zero() -> Result<(), Box<dyn Error>>
{
    let approximate = policy(
        Mode::Approximate(Budget::new(f64::from_bits(1), 0.)?),
        false,
    )?;
    let result = project("5e-324", approximate)?;
    assert_eq!(result.value().to_bits(), 1);
    assert!(!result.exact());
    assert!(project("5e-324", policy(Mode::ExactOnly, false)?).is_err());
    assert!(matches!(
        project("1e-400", approximate),
        Err(projection::Error::Underflow)
    ));
    assert!(project("1e-320", approximate).is_ok());
    let token = format!("{}e-1023", num_bigint::BigUint::from(5u8).pow(1023));
    assert_eq!(
        project(&token, policy(Mode::ExactOnly, false)?)?
            .value()
            .to_bits(),
        1u64 << 51
    );
    Ok(())
}
#[test]
fn configuration_and_arithmetic_bounds_are_explicit() -> Result<(), Box<dyn Error>> {
    for (abs, rel) in [
        (f64::NAN, 0.),
        (f64::INFINITY, 0.),
        (-1., 0.),
        (0., f64::NEG_INFINITY),
        (0., -0.1),
    ] {
        assert!(Budget::new(abs, rel).is_err());
    }
    assert_eq!(Budget::new(-0., -0.)?, Budget::new(0., 0.)?);
    assert!(
        Policy::new(
            Mode::ExactOnly,
            PrecisionSource {
                revision: 0,
                hint: Hint::Absent
            },
            false,
            Limits::default()
        )
        .is_err()
    );
    for limits in [
        Limits {
            work: 0,
            ..Limits::default()
        },
        Limits {
            token_bytes: 4097,
            ..Limits::default()
        },
        Limits {
            exponent: 1025,
            ..Limits::default()
        },
        Limits {
            bytes: 0,
            ..Limits::default()
        },
    ] {
        assert!(
            Policy::new(
                Mode::ExactOnly,
                PrecisionSource {
                    revision: 1,
                    hint: Hint::Absent
                },
                false,
                limits
            )
            .is_err()
        );
    }
    for (token, limits) in [
        (
            "0.5000",
            Limits {
                token_bytes: 4,
                ..Limits::default()
            },
        ),
        (
            "1e-100",
            Limits {
                exponent: 20,
                ..Limits::default()
            },
        ),
        (
            "0.5",
            Limits {
                work: 1,
                ..Limits::default()
            },
        ),
        (
            "0.5",
            Limits {
                bytes: 1,
                ..Limits::default()
            },
        ),
    ] {
        let p = Policy::new(
            Mode::ExactOnly,
            PrecisionSource {
                revision: 1,
                hint: Hint::Absent,
            },
            false,
            limits,
        )?;
        assert!(
            matches!(project(token, p), Err(projection::Error::ResourceLimit)),
            "{token}"
        );
    }
    assert!(
        project(
            &format!("0.{}1", "0".repeat(1024)),
            policy(Mode::ExactOnly, false)?
        )
        .is_err()
    );
    assert!(project("1e99999999999999999999999", policy(Mode::ExactOnly, false)?).is_err());
    Ok(())
}

#[test]
fn retained_plain_unrepresentable_integer_cannot_bypass_the_variant_fence()
-> Result<(), Box<dyn Error>> {
    let p = policy(Mode::Approximate(Budget::new(f64::MAX, 1.)?), true)?;
    assert!(matches!(
        projection::project(&Number::Verbatim("18446744073709551616".into()), p),
        Err(projection::Error::Unrepresentable)
    ));
    Ok(())
}

#[test]
fn negative_errors_and_near_zero_use_exact_magnitude_budgets() -> Result<(), Box<dyn Error>> {
    let relative = policy(Mode::Approximate(Budget::new(0., 2f64.powi(-54))?), false)?;
    let negative = project("-0.1", relative)?;
    fraction_is(negative.absolute_error(), 1, 180_143_985_094_819_840)?;
    fraction_is(
        negative.relative_error().ok_or("relative")?,
        1,
        18_014_398_509_481_984,
    )?;
    assert_eq!(negative.value(), -0.1);
    // The first subnormal is ~1.19% below decimal 5e-324. Relative budgets
    // remain meaningful here; no division by an f64 near-zero value occurs.
    assert!(
        project(
            "5e-324",
            policy(Mode::Approximate(Budget::new(0., 0.02)?), false)?
        )
        .is_ok()
    );
    assert!(
        project(
            "5e-324",
            policy(Mode::Approximate(Budget::new(0., 0.01)?), false)?
        )
        .is_err()
    );
    let protected = Policy::new(
        Mode::ExactOnly,
        PrecisionSource {
            revision: 1,
            hint: Hint::HighPrecision,
        },
        true,
        Limits::default(),
    )?;
    for number in [
        Number::I64(42),
        Number::U64(u64::MAX),
        Number::Verbatim("0.5".into()),
    ] {
        assert!(matches!(
            projection::project(&number, protected),
            Err(projection::Error::Protected)
        ));
    }
    Ok(())
}
