use agw_diff::DriftClass;
use agw_inspector::{Number, Precision, Value};
use agw_stats::field::{
    CharacterClass as C, Error, Format, Reason, Sentinel, Type, charset, coercion, completeness,
    profile,
};

fn number(token: &str) -> Value {
    Value::Number(Number::read(token, Precision::Observed))
}
fn samples(value: &Value, count: usize) -> Vec<Option<&Value>> {
    vec![Some(value); count]
}

#[test]
fn completeness_and_sentinels_have_independent_rates() -> Result<(), Error> {
    let null = Value::Null;
    let zero = number("0");
    let empty = Value::String(String::new());
    let nonzero = number("1");
    let mut before = vec![None; 5];
    before.extend(samples(&null, 5));
    before.extend(samples(&nonzero, 10));
    let mut after = samples(&zero, 10);
    after.extend(samples(&empty, 10));
    let r = completeness("amount", &before, &after)?;
    assert!(r.sufficient());
    assert_eq!(r.path(), "amount");
    assert_eq!(r.class(), Some(DriftClass::Semantic));
    assert_eq!(r.baseline().opportunities(), 20);
    assert_eq!(r.baseline().present(), 15);
    assert_eq!(r.baseline().non_null(), 10);
    assert_eq!(r.baseline().completeness(), 0.5);
    assert_eq!(r.observed().completeness(), 1.);
    assert!(
        matches!(r.reasons()[0], Reason::CompletenessDelta { rates } if rates.baseline == 0.5 && rates.observed == 1.)
    );
    for sentinel in [
        Sentinel::Missing,
        Sentinel::Null,
        Sentinel::Zero,
        Sentinel::EmptyString,
    ] {
        assert!(
            r.reasons()
                .iter()
                .any(|r| matches!(r, Reason::SentinelShift { sentinel: s, .. } if *s == sentinel))
        );
    }
    assert_eq!(before[0], None);
    assert!(std::ptr::eq(
        after[0].unwrap_or_else(|| panic!("sample")),
        &zero
    ));
    Ok(())
}

#[test]
fn mathematical_zero_and_type_profiles_are_lossless() -> Result<(), Error> {
    let zeros: Vec<_> = ["0", "-0", "0.0", "-0.00e-999", "0e400"].map(number).into();
    let other: Vec<_> = ["1e-400", "1e400", "184467440737095516160"]
        .map(number)
        .into();
    let boolean = Value::Bool(false);
    let null = Value::Null;
    let empty = Value::String("".into());
    let array = Value::Array(vec![number("0")]);
    let object = Value::Object(vec![]);
    let mut window: Vec<_> = zeros.iter().chain(&other).map(Some).collect();
    window.extend([
        Some(&boolean),
        Some(&null),
        Some(&empty),
        Some(&array),
        Some(&object),
        None,
    ]);
    let p = profile(&window)?;
    assert_eq!(p.sentinel_count(Sentinel::Zero), 5);
    assert_eq!(p.sentinel_count(Sentinel::Null), 1);
    assert_eq!(p.sentinel_count(Sentinel::Missing), 1);
    assert_eq!(p.sentinel_count(Sentinel::EmptyString), 1);
    assert_eq!(p.type_count(Type::Integer), 3);
    assert_eq!(p.type_count(Type::Number), 5);
    assert_eq!(p.type_count(Type::Boolean), 1);
    assert_eq!(p.type_count(Type::Array), 1);
    assert_eq!(p.type_count(Type::Object), 1);
    assert_eq!(other[2], number("184467440737095516160"));
    Ok(())
}

#[test]
fn completeness_floor_is_inclusive_even_when_float_subtraction_would_round_down()
-> Result<(), Error> {
    let n = number("1");
    let mut before = samples(&n, 16);
    before.extend(vec![None; 4]);
    for (present, semantic) in [(12, true), (13, false)] {
        let mut after = samples(&n, present);
        after.extend(vec![None; 20 - present]);
        assert_eq!(
            completeness("x", &before, &after)?.class().is_some(),
            semantic
        );
    }
    assert!(
        completeness("optional", &[None; 20], &samples(&n, 20))?
            .class()
            .is_some()
    );
    Ok(())
}

#[test]
fn type_consistency_requires_dominance_and_directional_degradation() -> Result<(), Error> {
    let n = number("1");
    let s = Value::String("1".into());
    let mut baseline = samples(&n, 18);
    baseline.extend(samples(&s, 2));
    for (numbers, semantic) in [(14, true), (15, false), (20, false)] {
        let mut after = samples(&n, numbers);
        after.extend(samples(&s, 20 - numbers));
        let r = coercion("x", &baseline, &after)?;
        assert_eq!(r.class().is_some(), semantic);
        if semantic {
            assert!(
                matches!(r.reasons()[0], Reason::TypeConsistency { kind: Type::Integer, rates }
                if rates.baseline == 0.9 && rates.observed == 0.7)
            );
        }
    }
    let mut mixed = samples(&n, 17);
    mixed.extend(samples(&s, 3));
    assert!(
        coercion("x", &mixed, &samples(&s, 20))?
            .reasons()
            .is_empty()
    );
    let mut sparse = samples(&n, 19);
    sparse.extend([None; 20]);
    assert!(!coercion("x", &sparse, &samples(&s, 20))?.sufficient());
    Ok(())
}

#[test]
fn iso_to_month_name_change_has_typed_charset_and_format_evidence() -> Result<(), Error> {
    let iso = Value::String("2026-01-05T12:30:00.123Z".into());
    let month = Value::String("Jan 5".into());
    let r = charset("created_at", &samples(&iso, 20), &samples(&month, 20))?;
    assert_eq!(r.class(), Some(DriftClass::Semantic));
    assert!(r.reasons().iter().any(
        |r| matches!(r, Reason::FormatEmerged { format: Format::MonthDay, pattern, .. }
        if pattern.starts_with('^') && pattern.ends_with('$'))
    ));
    assert!(r.reasons().iter().any(|r| matches!(
        r,
        Reason::CharsetShift {
            class: C::Whitespace,
            ..
        }
    )));
    let p = r
        .baseline()
        .fingerprint()
        .unwrap_or_else(|| panic!("fingerprint"));
    assert_eq!(p.format_count(Format::IsoTimestamp), 20);
    assert_eq!(p.class_count(C::AsciiLetter), 20);
    assert_eq!(p.classes().len(), 3);
    Ok(())
}

#[test]
fn format_recognition_is_anchored_ascii_and_lexical() -> Result<(), Error> {
    for (text, expected) in [
        ("2026-99-99", Format::IsoDate),
        ("2026-01-051", Format::Unknown),
        ("2026-01-05T01:02:03+05:30", Format::IsoTimestamp),
        ("2026-01-05T01:02:03Z", Format::IsoTimestamp),
        ("123", Format::Digits),
        ("Jan 05", Format::MonthDay),
        ("Jan 005", Format::Unknown),
        ("prefix Jan 5", Format::Unknown),
        ("123x", Format::Unknown),
        ("２０２６-01-05", Format::Unknown),
        ("", Format::Unknown),
        ("2026-01-05T01:02:03.Z", Format::Unknown),
        ("2026-01-05T01:02:03Zextra", Format::Unknown),
    ] {
        let v = Value::String(text.into());
        let p = profile(&samples(&v, 1))?;
        assert_eq!(
            p.fingerprint()
                .unwrap_or_else(|| panic!("fingerprint"))
                .format_count(expected),
            1,
            "{text}"
        );
    }
    Ok(())
}

#[test]
fn unicode_control_and_replacement_are_distinct_classes() -> Result<(), Error> {
    let v = Value::String("a19 !\té�".into());
    let p = profile(&samples(&v, 20))?;
    let fp = p.fingerprint().unwrap_or_else(|| panic!("fingerprint"));
    for c in [
        C::AsciiLetter,
        C::AsciiDigit,
        C::Whitespace,
        C::Punctuation,
        C::Control,
        C::NonAscii,
        C::Replacement,
    ] {
        assert_eq!(fp.class_count(c), 20);
    }
    let c1 = Value::String("\u{0085}".into());
    let p = profile(&samples(&c1, 1))?;
    let fp = p.fingerprint().unwrap_or_else(|| panic!("fingerprint"));
    assert_eq!(fp.class_count(C::Control), 1);
    assert_eq!(fp.class_count(C::NonAscii), 0);
    let a = Value::String("hello".into());
    let b = Value::String("world".into());
    assert!(
        charset("x", &samples(&a, 20), &samples(&b, 20))?
            .reasons()
            .is_empty()
    );
    assert!(
        charset(
            "x",
            &samples(&a, 20),
            &samples(&Value::String("�".into()), 20)
        )?
        .class()
        .is_some()
    );
    Ok(())
}

#[test]
fn emergence_and_class_boundaries_are_material_and_inclusive() -> Result<(), Error> {
    let known = Value::String("abc".into());
    let digits = Value::String("123".into());
    let mut before = samples(&known, 19);
    before.extend(samples(&digits, 1));
    let mut after = samples(&known, 16);
    after.extend(samples(&digits, 4));
    let r = charset("x", &before, &after)?;
    assert_eq!(r.reasons().len(), 1);
    assert!(matches!(
        r.reasons()[0],
        Reason::FormatEmerged {
            format: Format::Digits,
            ..
        }
    ));
    let mut before = samples(&known, 18);
    before.extend(samples(&digits, 2));
    assert!(charset("x", &before, &after)?.reasons().is_empty());
    let mut after = samples(&known, 17);
    after.extend(samples(&digits, 3));
    assert!(
        charset("x", &samples(&known, 20), &after)?
            .reasons()
            .is_empty()
    );
    let mut after = samples(&known, 16);
    after.extend(samples(&digits, 4));
    assert!(
        charset("x", &samples(&known, 20), &after)?
            .reasons()
            .iter()
            .any(|r| matches!(r, Reason::CharsetShift { .. }))
    );
    Ok(())
}

#[test]
fn insufficient_and_invalid_windows_do_not_produce_partial_findings() {
    let s = Value::String("a".into());
    let long = Value::String("x".repeat(4097));
    for detector in [completeness, coercion, charset] {
        let r = detector("x", &[], &samples(&s, 20)).unwrap_or_else(|e| panic!("{e}"));
        assert!(!r.sufficient());
        assert!(r.reasons().is_empty());
        assert_eq!(r.class(), None);
        let short =
            detector("x", &samples(&s, 19), &samples(&s, 20)).unwrap_or_else(|e| panic!("{e}"));
        assert!(!short.sufficient());
        assert!(short.reasons().is_empty());

        assert_eq!(detector("", &[], &[]).err(), Some(Error::InvalidInput));
        assert_eq!(
            detector("x", &[], &samples(&s, 4097)).err(),
            Some(Error::ResourceLimit)
        );
        assert_eq!(
            detector("x", &[], &samples(&long, 1)).err(),
            Some(Error::ResourceLimit)
        );
        assert_eq!(
            detector(&"x".repeat(4097), &[], &[]).err(),
            Some(Error::ResourceLimit)
        );
        let text = Value::String("x".repeat(4096));
        assert_eq!(
            detector("x", &samples(&text, 128), &samples(&text, 128)).err(),
            Some(Error::ResourceLimit)
        );
        for token in ["junk", "true", "\"0\"", "0 ", "01"] {
            assert_eq!(
                detector("x", &[], &samples(&number(token), 1)).err(),
                Some(Error::InvalidInput)
            );
        }
    }

    let null = Value::Null;
    let mut sparse_strings = samples(&s, 19);
    sparse_strings.extend(samples(&null, 20));
    assert!(
        !charset("x", &sparse_strings, &samples(&s, 20))
            .unwrap_or_else(|e| panic!("{e}"))
            .sufficient()
    );
    assert_eq!(
        profile(&samples(&s, 4096)).map(|p| p.opportunities()),
        Ok(4096)
    );
}
