//! TI-07 — numeric fidelity: integers and decimals survive a round trip.
//!
//! Contract §3.1: re-serializing JSON in JavaScript destroys large integers via
//! float64 rounding, so a "repaired" charge ID can come back as a *different* ID.
//! In Rust that failure is a choice, not a language trait — and these cases pin
//! the choice. The load-bearing property is [`round_trips`]: for every lexeme the
//! reader accepts, writing it back produces the identical text. A reserializer
//! that changes `-0` to `0` or `18446744073709551615` to `…551520` fails here.

use agw_inspector::numeric::{Number, Precision};

/// The number, written back out, must be the number that arrived.
fn round_trips(lexeme: &str, precision: Precision) -> String {
    Number::read(lexeme, precision).lexeme().into_owned()
}

fn read(lexeme: &str) -> Number {
    Number::read(lexeme, Precision::HighPrecision)
}

// ---------------------------------------------------------------- the fast path

#[test]
fn reads_an_ordinary_integer_as_i64() {
    assert_eq!(read("42"), Number::I64(42));
    assert_eq!(read("-42"), Number::I64(-42));
    assert_eq!(read("0"), Number::I64(0));
}

#[test]
fn lexeme_length_matches_the_exact_lexeme_without_formatting() {
    for number in [
        Number::I64(0),
        Number::I64(-1),
        Number::I64(i64::MIN),
        Number::I64(i64::MAX),
        Number::U64(0),
        Number::U64(u64::MAX),
        Number::Verbatim("-0".into()),
        Number::Unrepresentable("1e400".into()),
    ] {
        assert_eq!(number.lexeme_len(), number.lexeme().len(), "{number:?}");
    }
}

#[test]
fn reads_the_i64_boundaries_exactly() {
    assert_eq!(read("-9223372036854775808"), Number::I64(i64::MIN));
    assert_eq!(read("9223372036854775807"), Number::I64(i64::MAX));
}

#[test]
fn reads_above_i64_max_as_u64() {
    // One past `i64::MAX` is where a signed-only model starts truncating.
    assert_eq!(
        read("9223372036854775808"),
        Number::U64(9_223_372_036_854_775_808)
    );
    assert_eq!(read("18446744073709551615"), Number::U64(u64::MAX));
}

/// The 2^53 boundary — the largest integers a float64 can no longer name.
///
/// All three fit in `i64`, so the fast path reads each as `I64`. The claim under
/// test is that they stay *distinct*: a float64 round trip collapses 2^53-1 and
/// 2^53+1 onto one value, which is how a repaired ID becomes a different ID.
#[test]
fn reads_the_float64_boundary_as_three_distinct_integers() {
    let below = read("9007199254740991"); // 2^53 - 1
    let at = read("9007199254740992"); // 2^53
    let above = read("9007199254740993"); // 2^53 + 1

    assert_eq!(below, Number::I64(9_007_199_254_740_991));
    assert_eq!(at, Number::I64(9_007_199_254_740_992));
    assert_eq!(above, Number::I64(9_007_199_254_740_993));

    let all = [&below, &at, &above];
    for (index, left) in all.iter().enumerate() {
        for right in &all[index + 1..] {
            assert_ne!(left, right, "float64 cannot tell these apart");
        }
    }
}

/// Snowflake IDs are `u64::MAX` and its neighbours, and they arrive as integers.
#[test]
fn reads_snowflake_boundary_ids_exactly() {
    for lexeme in [
        "9223372036854775807", // i64::MAX — the last signed value
        "9223372036854775808", // one past it: no signed home left
        "18446744073709551614",
        "18446744073709551615", // u64::MAX
    ] {
        let number = read(lexeme);
        assert!(number.is_exact(), "{lexeme} is not modelled exactly");
        assert_eq!(number.lexeme(), lexeme, "{lexeme} did not survive");
    }

    assert_eq!(read("9223372036854775807"), Number::I64(i64::MAX));
    assert_eq!(read("18446744073709551615"), Number::U64(u64::MAX));
    assert_eq!(read("18446744073709551615").as_u64(), Some(u64::MAX));
}

#[test]
fn exposes_the_integer_value_on_both_paths() {
    assert_eq!(read("-7").as_i64(), Some(-7));
    assert_eq!(read("18446744073709551615").as_u64(), Some(u64::MAX));
    // A `u64` too large for `i64` is not an `i64`, and the converse.
    assert_eq!(read("18446744073709551615").as_i64(), None);
    assert_eq!(read("-1").as_u64(), None);
}

// ------------------------------------------------------------- wider than u64

#[test]
fn retains_an_oversized_integer_verbatim_when_flagged() {
    // Past `u64::MAX` there is no integer left to hold it.
    let lexeme = "340282366920938463463374607431768211456"; // 2^128
    assert_eq!(read(lexeme), Number::Verbatim(lexeme.into()));
    assert!(read(lexeme).is_exact());
}

#[test]
fn declines_to_model_an_oversized_integer_when_unflagged() {
    let lexeme = "340282366920938463463374607431768211456";
    let number = Number::read(lexeme, Precision::Observed);
    assert!(!number.is_exact(), "AGINDOW must not claim to model it");
    // Nothing is lost even when it declines: the lexeme is retained, so the
    // classifier can still see the value and the writer can still reproduce it.
    assert_eq!(number.lexeme(), lexeme);
}

/// The distinction that makes the flag worth having: same input, two answers.
#[test]
fn the_precision_flag_decides_an_oversized_integer() {
    let lexeme = "123456789012345678901234567890";
    assert!(Number::read(lexeme, Precision::HighPrecision).is_exact());
    assert!(!Number::read(lexeme, Precision::Observed).is_exact());
}

/// `Observed` is the default: a caller that has no contract to consult gets the
/// conservative answer, never a licence to be lossier.
#[test]
fn observed_is_the_default_precision() {
    assert_eq!(Precision::default(), Precision::Observed);
    assert!(!Number::read("123456789012345678901234567890", Precision::default()).is_exact());
}

// ------------------------------------------------------------- decimals, floats

#[test]
fn retains_decimals_and_exponents_verbatim() {
    for lexeme in [
        "0.1",
        "-0.00000000000000000001",
        "1.7976931348623157e308",  // f64::MAX
        "2.2250738585072014e-308", // f64::MIN_POSITIVE
        "5e-324",                  // f64::MIN_POSITIVE subnormal
        "1e400",                   // overflows f64 to infinity
        "1234567890.12345678901234567890",
    ] {
        assert_eq!(read(lexeme), Number::Verbatim(lexeme.into()), "{lexeme}");
        assert!(read(lexeme).is_exact(), "{lexeme}");
    }
}

/// Precision only governs *integers*. A decimal is never routed through an
/// integer conversion, so there is nothing to truncate and the flag does not
/// apply — refusing `0.1` would be absurd.
#[test]
fn precision_does_not_apply_to_decimals() {
    for lexeme in ["0.1", "1.5e10", "123.456"] {
        assert!(
            Number::read(lexeme, Precision::Observed).is_exact(),
            "{lexeme}"
        );
        assert!(
            Number::read(lexeme, Precision::HighPrecision).is_exact(),
            "{lexeme}"
        );
    }
}

// ----------------------------------------------------------------- the round trip

/// Contract §3.1: writing a value back must not change it. This is the property
/// the whole crate exists for, and it holds for every lexeme — `-0` included.
#[test]
fn round_trips_every_case_verbatim() {
    let cases = [
        "0",
        "-0",
        "42",
        "-42",
        "007",
        "-00",
        "9223372036854775807",
        "-9223372036854775808",
        "18446744073709551615",
        "18446744073709551614",
        "9007199254740991",
        "9007199254740992",
        "9007199254740993",
        "340282366920938463463374607431768211456",
        "0.1",
        "-0.1",
        "1e400",
        "1.7976931348623157e308",
        "5e-324",
        "1234567890.12345678901234567890",
    ];
    for lexeme in cases {
        for precision in [Precision::HighPrecision, Precision::Observed] {
            assert_eq!(
                round_trips(lexeme, precision),
                lexeme,
                "{lexeme} @ {precision:?}"
            );
        }
    }
}

/// `-0` is JSON's spelling of the float negative zero. Reading it as an integer
/// would round-trip it to `0` — the exact reserializer bug this crate prevents —
/// so it is kept verbatim.
#[test]
fn keeps_negative_zero_verbatim() {
    assert_eq!(read("-0"), Number::Verbatim("-0".into()));
    assert_ne!(read("-0"), read("0"));
    assert_eq!(round_trips("-0", Precision::Observed), "-0");
}

/// A JSON number never carries a leading `+`, a leading zero, or a bare `.5`.
/// Non-canonical text still round-trips rather than being silently normalised.
#[test]
fn non_canonical_integer_text_is_kept_verbatim() {
    assert_eq!(round_trips("007", Precision::Observed), "007");
    assert_eq!(round_trips("-00", Precision::Observed), "-00");
}

use proptest::prelude::*;

// The repair path re-serializes, and contract §3.1 allows that to stop being
// byte-identical but never allows it to change a number. `Serialize` writes the
// lexeme, so that is a property of the type rather than a rule every future call
// site has to remember.
#[test]
fn serializes_to_the_source_lexeme() -> Result<(), serde_json::Error> {
    #[derive(serde::Serialize)]
    struct Charge {
        id: Number,
        amount: Number,
        tax: Number,
    }

    for (id, amount, tax) in [
        ("18446744073709551615", "0.1", "1.7976931348623157e308"),
        ("9007199254740993", "-0", "5e-324"),
        (
            "9223372036854775808",
            "1234567890.12345678901234567890",
            "1e400",
        ),
    ] {
        let charge = Charge {
            id: read(id),
            amount: read(amount),
            tax: read(tax),
        };
        let json = serde_json::to_string(&charge)?;
        assert_eq!(
            json,
            format!(r#"{{"id":{id},"amount":{amount},"tax":{tax}}}"#),
            "{id}"
        );
    }
    Ok(())
}

// Any 64-bit integer, written the only way JSON can, comes back identical and is
// modelled exactly — the fast path is exact, not merely close.
proptest! {
    #![proptest_config(ProptestConfig { cases: 512, ..ProptestConfig::default() })]

    #[test]
    fn any_i64_round_trips_and_is_exact(value in any::<i64>()) {
        let lexeme = value.to_string();
        let number = Number::read(&lexeme, Precision::Observed);
        prop_assert_eq!(&number, &Number::I64(value));
        prop_assert!(number.is_exact());
        prop_assert_eq!(number.lexeme(), lexeme);
        prop_assert_eq!(number.as_i64(), Some(value));
    }

    #[test]
    fn any_u64_above_i64_max_round_trips_and_is_exact(
        value in (i64::MAX as u64 + 1)..=u64::MAX,
    ) {
        let lexeme = value.to_string();
        let number = Number::read(&lexeme, Precision::Observed);
        prop_assert_eq!(&number, &Number::U64(value));
        prop_assert!(number.is_exact());
        prop_assert_eq!(number.lexeme(), lexeme);
    }

    /// The property that makes the type worth having: whatever lexeme the reader
    /// is handed — integer, decimal, exponent, oversized, non-canonical — writing
    /// it back reproduces it byte for byte.
    #[test]
    fn any_number_token_round_trips(bytes in proptest::collection::vec(any::<u8>(), 1..24)) {
        let Ok(lexeme) = std::str::from_utf8(&bytes) else { return Ok(()) };
        if !serde_json::from_str::<serde_json::Number>(lexeme).is_ok() { return Ok(()) }
        for precision in [Precision::HighPrecision, Precision::Observed] {
            prop_assert_eq!(round_trips(lexeme, precision), lexeme);
        }
    }
}

/// TI-10 settles the inherited question: accessors expose variants, not
/// convertible values. In particular a positive signed integer is still I64.
#[test]
fn integer_accessors_are_variant_accessors() {
    assert_eq!(Number::I64(5).as_u64(), None);
    assert_eq!(Number::U64(5).as_i64(), None);
    assert_eq!(Number::I64(0).as_u64(), None);
    assert_eq!(Number::I64(-1).as_u64(), None);
    assert_eq!(Number::I64(i64::MIN).as_i64(), Some(i64::MIN));
    assert_eq!(Number::U64(u64::MAX).as_u64(), Some(u64::MAX));
    for number in [
        Number::Verbatim("5".into()),
        Number::Unrepresentable("5".into()),
    ] {
        assert_eq!(number.as_i64(), None);
        assert_eq!(number.as_u64(), None);
    }
}
