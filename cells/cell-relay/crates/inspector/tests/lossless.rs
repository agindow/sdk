//! TI-39: wire-token fidelity through the real parser and serializer.
#[path = "support/lossless.rs"]
mod support;

use agw_inspector::{Number, Parse, ParseClass, ParseFailure, Precision, Tee, Value, inspect};
use proptest::prelude::*;
use std::io::{Cursor, Read};
use support::{document, lexemes, number_token, tree_lexemes};

fn parsed(bytes: &[u8], precision: Option<Precision>) -> Value {
    let Parse::Parsed(value) = inspect(bytes, false, precision).parse else {
        panic!(
            "valid document was not parsed: {}",
            String::from_utf8_lossy(bytes)
        );
    };
    value
}

#[test]
fn edge_tokens_survive_in_nested_values_and_duplicate_keys() -> Result<(), serde_json::Error> {
    let bytes = br#" {"id":9007199254740993,"id":18446744073709551615,"nested":[1e400,-0,0.1,1234567890.12345678901234567890,18446744073709551616],"text":"123 \" 4e2"} "#;
    let tree = parsed(bytes, None);
    assert_eq!(tree_lexemes(&tree), lexemes(bytes));
    let serialized = serde_json::to_vec(&tree)?;
    assert_eq!(lexemes(&serialized), lexemes(bytes));
    assert_eq!(parsed(&serialized, None), tree);
    Ok(())
}

#[test]
fn missing_precision_is_observed_and_high_precision_is_explicit() {
    let token = b"18446744073709551616";
    assert_eq!(
        parsed(token, None),
        Value::Number(Number::Unrepresentable("18446744073709551616".into()))
    );
    assert_eq!(
        parsed(token, Some(Precision::Observed)),
        parsed(token, None)
    );
    assert_eq!(
        parsed(token, Some(Precision::HighPrecision)),
        Value::Number(Number::Verbatim("18446744073709551616".into()))
    );
}

#[test]
fn tee_preserves_upstream_bytes_and_supplies_precision_after_eof()
-> Result<(), Box<dyn std::error::Error>> {
    let bytes = br#"{"id":18446744073709551616,"tiny":1e-400}"#;
    for precision in [None, Some(Precision::HighPrecision)] {
        let mut tee = Tee::new(Cursor::new(bytes));
        let mut client = Vec::new();
        tee.read_to_end(&mut client)?;
        assert_eq!(client, bytes);
        let inspected = match precision {
            None => tee.into_inspection(),
            Some(p) => tee.into_inspection_with_precision(p),
        };
        assert!(!inspected.truncated);
        assert_eq!(inspected.parse, Parse::Parsed(parsed(bytes, precision)));
    }
    Ok(())
}

#[test]
fn truncated_and_invalid_numbers_remain_classification_inputs() {
    let result = inspect(b"1e400", true, None);
    assert!(result.truncated);
    assert!(matches!(result.parse, Parse::Parsed(_)));
    for bytes in [
        b"01".as_slice(),
        b"NaN",
        b"Infinity",
        b"-",
        b"1e",
        b"1 2",
        br#"[1,]"#,
    ] {
        assert!(matches!(
            inspect(bytes, false, None).parse,
            Parse::Unparseable(_)
        ));
    }
    let Parse::Unparseable(failure) = inspect(b"1e", true, None).parse else {
        panic!("incomplete exponent parsed")
    };
    // Keep the pre-TI-39 parser's Eof vocabulary for this same capture.
    let Err(original) = serde_json::from_slice::<serde_json::Value>(b"1e") else {
        panic!("old parser accepted an incomplete exponent")
    };
    assert_eq!(failure.class, ParseFailure::from(&original).class);
    assert_eq!(failure.class, ParseClass::Eof);
}

#[test]
fn excessive_nesting_is_a_recorded_failure() {
    let accepted = format!("{}0{}", "[".repeat(127), "]".repeat(127));
    assert!(matches!(
        inspect(accepted.as_bytes(), false, None).parse,
        Parse::Parsed(_)
    ));
    let bytes = format!("{}0{}", "[".repeat(128), "]".repeat(128));
    let Parse::Unparseable(failure) = inspect(bytes.as_bytes(), false, None).parse else {
        panic!("nesting limit not enforced")
    };
    assert_eq!(failure.class, ParseClass::Data);
}

#[test]
fn non_numeric_values_decode_and_serialize_as_json() -> Result<(), serde_json::Error> {
    let bytes = br#"[null,true,false,"\u00e9\n\"\\123",{},[]]"#;
    let tree = parsed(bytes, None);
    assert_eq!(
        tree,
        Value::Array(vec![
            Value::Null,
            Value::Bool(true),
            Value::Bool(false),
            Value::String("é\n\"\\123".into()),
            Value::Object(vec![]),
            Value::Array(vec![])
        ])
    );
    let written = serde_json::to_vec(&tree)?;
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&written)?,
        serde_json::from_slice::<serde_json::Value>(bytes)?
    );
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 512, ..ProptestConfig::default() })]

    #[test]
    fn any_numeric_token_reaches_the_tree_unchanged(token in number_token()) {
        for precision in [None, Some(Precision::HighPrecision)] {
            let tree = parsed(token.as_bytes(), precision);
            prop_assert_eq!(tree_lexemes(&tree), vec![token.clone()]);
        }
    }

    #[test]
    fn any_generated_document_preserves_every_numeric_token(doc in document()) {
        let expected = lexemes(doc.as_bytes());
        for precision in [None, Some(Precision::HighPrecision)] {
            let tree = parsed(doc.as_bytes(), precision);
            prop_assert_eq!(tree_lexemes(&tree), expected.clone());
            let written = serde_json::to_vec(&tree)?;
            prop_assert_eq!(lexemes(&written), expected.clone());
            prop_assert_eq!(parsed(&written, precision), tree);
        }
    }

    #[test]
    fn arbitrary_bytes_never_panic_and_every_accepted_number_survives(bytes in proptest::collection::vec(any::<u8>(), 0..1024), truncated in any::<bool>()) {
        let result = inspect(&bytes, truncated, None);
        prop_assert_eq!(result.truncated, truncated);
        if let Parse::Parsed(tree) = result.parse {
            prop_assert_eq!(tree_lexemes(&tree), lexemes(&bytes));
            let written = serde_json::to_vec(&tree)?;
            prop_assert_eq!(lexemes(&written), lexemes(&bytes));
        }
    }

    #[test]
    fn arbitrary_bytes_in_an_id_token_preserve_u64_boundaries(bytes in any::<[u8; 8]>()) {
        let id = u64::from_le_bytes(bytes);
        let doc = format!("{{\"id\":{id}}}");
        prop_assert_eq!(tree_lexemes(&parsed(doc.as_bytes(), None)), vec![id.to_string()]);
    }
}

#[test]
fn unescaped_strings_keep_their_utf8_bytes() {
    // The common case for large payloads is a string with no backslash. It must
    // decode to the same text whether copied verbatim or parsed by serde_json.
    let long = "x".repeat(64 * 1024);
    let document = format!(r#"{{"long":"{long}","plain":"héllo 世界"}}"#);
    let tree = parsed(document.as_bytes(), None);
    let Value::Object(entries) = tree else {
        panic!("object did not parse");
    };
    assert_eq!(entries[0].1, Value::String(long));
    assert_eq!(entries[1].1, Value::String("héllo 世界".into()));
}

#[test]
fn unfinished_numeric_tokens_keep_existing_error_categories() {
    for bytes in [
        b"-".as_slice(),
        b"1.",
        b"1e",
        b"1e+",
        b"[1e-",
        br#"{"n":1e"#,
        b"1e ",
        b"-e",
        b"1e++",
    ] {
        let Err(original) = serde_json::from_slice::<serde_json::Value>(bytes) else {
            panic!("invalid fixture")
        };
        let Parse::Unparseable(actual) = inspect(bytes, false, None).parse else {
            panic!("invalid input parsed")
        };
        assert_eq!(
            actual.class,
            ParseFailure::from(&original).class,
            "{:?}",
            bytes
        );
    }
}

#[test]
fn nested_string_failures_retain_capture_coordinates() {
    for bytes in [
        br#"{"s":"\uD800"}"#.as_slice(),
        b"{\n \"nested\": [\n \"\\uD800\" ]}",
    ] {
        let Err(original) = serde_json::from_slice::<serde_json::Value>(bytes) else {
            panic!("invalid fixture")
        };
        let Parse::Unparseable(actual) = inspect(bytes, false, None).parse else {
            panic!("invalid string parsed")
        };
        let expected = ParseFailure::from(&original);
        assert_eq!(
            (actual.class, actual.line, actual.column),
            (expected.class, expected.line, expected.column)
        );
    }
}
