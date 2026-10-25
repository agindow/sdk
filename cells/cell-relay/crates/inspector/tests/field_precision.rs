//! TI-38: repeated overflow evidence and exact field-local interpretation.
use agw_inspector::inference::{BaselineStore, JsonType, Key, Limits, SpecPrior};
use agw_inspector::{FieldPrecisions, Number, Parse, Value, inspect_fields};
use std::{error::Error, num::NonZeroUsize};
type Result = std::result::Result<(), Box<dyn Error>>;

fn number<'a>(value: &'a Value, key: &str) -> std::result::Result<&'a Number, Box<dyn Error>> {
    let Value::Object(entries) = value else {
        return Err("expected object".into());
    };
    match entries.iter().find(|(k, _)| k == key) {
        Some((_, Value::Number(n))) => Ok(n),
        _ => Err("expected number".into()),
    }
}

#[test]
fn precision_is_field_local_and_never_changes_numeric_lexemes() -> Result {
    let fields = FieldPrecisions::new([
        ("id", Some(true)),
        ("off", Some(false)),
        ("small", Some(true)),
    ])?;
    let bytes = br#"{"id":18446744073709551616,"other":18446744073709551616,"off":18446744073709551616,"small":7}"#;
    let parsed = inspect_fields(bytes, false, &fields);
    let Parse::Parsed(value) = parsed.parse else {
        return Err("expected parsed".into());
    };
    assert!(matches!(number(&value, "id")?, Number::Verbatim(_)));
    assert!(matches!(
        number(&value, "other")?,
        Number::Unrepresentable(_)
    ));
    assert!(matches!(number(&value, "off")?, Number::Unrepresentable(_)));
    assert_eq!(number(&value, "small")?, &Number::I64(7));
    assert_eq!(serde_json::to_vec(&value)?, bytes);
    Ok(())
}

#[test]
fn paths_distinguish_literals_empty_keys_and_pooled_array_items() -> Result {
    let fields = FieldPrecisions::new([
        ("rows[].id", Some(true)),
        ("a\\.b", Some(true)),
        ("\\e", Some(true)),
        ("container", Some(true)),
    ])?;
    let bytes = br#"{"rows":[{"id":18446744073709551616},{"id":18446744073709551616}],"a.b":18446744073709551616,"":18446744073709551616,"a":{"b":18446744073709551616},"container":{"id":18446744073709551616}}"#;
    let Parse::Parsed(Value::Object(entries)) = inspect_fields(bytes, true, &fields).parse else {
        return Err("expected object".into());
    };
    for (key, value) in entries {
        match key.as_str() {
            "rows" => {
                let Value::Array(rows) = value else {
                    return Err("array".into());
                };
                for row in rows {
                    assert!(matches!(number(&row, "id")?, Number::Verbatim(_)));
                }
            }
            "a.b" | "" => assert!(matches!(value, Value::Number(Number::Verbatim(_)))),
            "a" => assert!(matches!(number(&value, "b")?, Number::Unrepresentable(_))),
            "container" => assert!(matches!(number(&value, "id")?, Number::Unrepresentable(_))),
            _ => return Err("unexpected key".into()),
        }
    }
    let root = FieldPrecisions::new([("$", Some(true))])?;
    assert!(matches!(
        inspect_fields(b"18446744073709551616", false, &root).parse,
        Parse::Parsed(Value::Number(Number::Verbatim(_)))
    ));
    Ok(())
}

#[test]
fn invalid_or_ambiguous_precision_maps_are_rejected_before_inspection() {
    assert!(FieldPrecisions::new([("", Some(true))]).is_err());
    assert!(FieldPrecisions::new([("id", Some(true)), ("id", Some(false))]).is_err());
    let fields: Vec<_> = (0..4097).map(|n| (format!("f{n}"), Some(true))).collect();
    assert!(FieldPrecisions::new(fields.iter().map(|(p, f)| (p.as_str(), *f))).is_err());
    let huge = "x".repeat(1024 * 1024 + 1);
    assert!(FieldPrecisions::new([(huge.as_str(), Some(true))]).is_err());
}

#[test]
fn repeated_overflow_counts_samples_not_items_or_spec_priors() -> Result {
    let mut store = BaselineStore::new(NonZeroUsize::new(3).ok_or("count")?, Limits::default());
    let key = Key::new("vendor", "call");
    store.begin(
        key.clone(),
        Some(SpecPrior::object([(
            "id".into(),
            SpecPrior::scalar(JsonType::Integer),
        )])),
    )?;
    for bytes in [
        br#"{"id":18446744073709551616,"once":18446744073709551616,"rows":[1,18446744073709551616,18446744073709551616]}"#.as_slice(),
        br#"{"id":-9223372036854775809,"once":1,"rows":[1]}"#,
        br#"{"id":18446744073709551615,"once":2,"rows":[1]}"#,
    ] { store.observe(&key, bytes, false)?; }
    let schema = &store.latest(&key).ok_or("baseline")?.schema;
    let id = schema.properties.get("id").ok_or("id")?;
    assert_eq!(id.high_precision, Some(true));
    assert_eq!(id.overflow_samples, 2);
    assert_eq!(
        schema.properties.get("once").ok_or("once")?.high_precision,
        None
    );
    let rows = schema
        .properties
        .get("rows")
        .and_then(|s| s.items.as_deref())
        .ok_or("rows")?;
    assert_eq!(rows.overflow_samples, 1);
    assert_eq!(rows.high_precision, None);
    assert_eq!(
        schema.json_schema()["properties"]["id"]["high_precision"],
        true
    );
    Ok(())
}

#[test]
fn a_single_overflow_or_noninteger_lexeme_never_infers_a_flag() -> Result {
    for bytes in [
        b"18446744073709551616".as_slice(),
        b"1e400",
        b"0.1234567890123456789",
        b"-0",
    ] {
        let mut store = BaselineStore::new(NonZeroUsize::new(1).ok_or("count")?, Limits::default());
        let key = Key::new("vendor", "call");
        store.begin(key.clone(), None)?;
        store.observe(&key, bytes, false)?;
        assert_eq!(
            store.latest(&key).ok_or("baseline")?.schema.high_precision,
            None
        );
    }
    Ok(())
}

#[test]
fn rejected_samples_and_relearning_do_not_leak_precision_evidence() -> Result {
    let key = Key::new("vendor", "call");
    let mut store = BaselineStore::new(NonZeroUsize::new(2).ok_or("count")?, Limits::default());
    store.begin(key.clone(), None)?;
    assert!(store.observe(&key, b"18446744073709551616", true).is_err());
    assert!(
        store
            .observe(&key, b"18446744073709551616 trailing", false)
            .is_err()
    );
    store.observe(&key, b"18446744073709551616", false)?;
    store.observe(&key, b"1", false)?;
    let first = &store.version(&key, 1).ok_or("first")?.schema;
    assert_eq!(first.overflow_samples, 1);
    assert_eq!(first.high_precision, None);
    store.begin(key.clone(), None)?;
    for _ in 0..2 {
        store.observe(&key, b"18446744073709551616", false)?;
    }
    assert_eq!(
        store.latest(&key).ok_or("second")?.schema.high_precision,
        Some(true)
    );
    assert_eq!(
        store.version(&key, 1).ok_or("first")?.schema.high_precision,
        None
    );
    Ok(())
}

#[test]
fn precision_paths_match_reader_escaping_without_changing_legacy_paths() -> Result {
    let key = Key::new("vendor", "call");
    let mut store = BaselineStore::new(NonZeroUsize::new(2).ok_or("count")?, Limits::default());
    store.begin(key.clone(), None)?;
    let bytes = br#"{"":18446744073709551616,"\\e":18446744073709551616}"#;
    for _ in 0..2 {
        store.observe(&key, bytes, false)?;
    }
    let schema = &store.latest(&key).ok_or("baseline")?.schema;
    assert!(schema.paths().contains_key(""));
    let paths = schema.precision_paths();
    assert!(paths.contains_key("\\e"));
    assert!(paths.contains_key("\\\\e"));
    let fields = FieldPrecisions::new(
        paths
            .iter()
            .map(|(path, node)| (path.as_str(), node.high_precision)),
    )?;
    let Parse::Parsed(value) = inspect_fields(bytes, false, &fields).parse else {
        return Err("parsed".into());
    };
    assert!(matches!(number(&value, "")?, Number::Verbatim(_)));
    assert!(matches!(number(&value, "\\e")?, Number::Verbatim(_)));
    Ok(())
}

#[test]
fn repeated_nonoverflow_tokens_supply_no_precision_evidence() -> Result {
    for bytes in [
        b"18446744073709551615".as_slice(),
        b"-9223372036854775808",
        b"1e400",
        b"0.1234567890123456789",
        b"-0",
    ] {
        let key = Key::new("vendor", "call");
        let mut store = BaselineStore::new(NonZeroUsize::new(2).ok_or("count")?, Limits::default());
        store.begin(key.clone(), None)?;
        for _ in 0..2 {
            store.observe(&key, bytes, false)?;
        }
        let schema = &store.latest(&key).ok_or("baseline")?.schema;
        assert_eq!(schema.overflow_samples, 0);
        assert_eq!(schema.high_precision, None);
    }
    Ok(())
}
