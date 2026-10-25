//! TI-06 — what structured inspection records about a captured head.
//!
//! Contract §4.4: an unparseable body is shape drift with no repair, and the
//! engine must never raise on it. Inspection therefore records parse failures
//! as data — the input a classifier reads — never as an error the client path
//! has to handle.

mod support;

use std::io;

use agw_inspector::{Number, Parse, ParseClass, ParseFailure, Tee, Value, inspect};
use support::{ChunkedReader, FailingReader, read_all};

#[test]
fn valid_json_parses_to_its_value() {
    let outcome = inspect(br#"{"amount":1200}"#, false, None);
    assert_eq!(
        outcome.parse,
        Parse::Parsed(Value::Object(vec![(
            "amount".into(),
            Value::Number(Number::I64(1200))
        ),]))
    );
    assert!(!outcome.truncated);
}

#[test]
fn a_parse_failure_is_recorded_not_raised() {
    let outcome = inspect(b"not json at all", false, None);
    let Parse::Unparseable(failure) = outcome.parse else {
        panic!("garbage parsed as JSON");
    };
    assert_eq!(failure.class, ParseClass::Syntax);
    assert!(!failure.message.is_empty());
    assert!(failure.line >= 1, "the line where parsing stopped");
    assert!(failure.column >= 1);
    assert!(!outcome.truncated);
}

#[test]
fn an_empty_capture_is_end_of_input() {
    let Parse::Unparseable(failure) = inspect(b"", false, None).parse else {
        panic!("an empty body parsed as JSON");
    };
    assert_eq!(failure.class, ParseClass::Eof);
}

#[test]
fn invalid_utf8_is_recorded_not_fatal() {
    let Parse::Unparseable(failure) = inspect(&[0xff, 0xfe, 0x00], false, None).parse else {
        panic!("non-UTF-8 bytes parsed as JSON");
    };
    assert_eq!(failure.class, ParseClass::Syntax);
}

#[test]
fn a_head_cut_by_the_cap_records_truncation_and_eof() -> io::Result<()> {
    // The first 16 bytes of a valid 48-byte document are all inspection gets;
    // the parse stops at the cut — which is our cap, not vendor drift. This is
    // exactly why `truncated` is recorded separately from the parse outcome.
    const PAYLOAD: &[u8] = br#"{"id":"evt_01HQ","amount":1200,"currency":"usd"}"#;
    let mut tee = Tee::with_cap(ChunkedReader::new(PAYLOAD, 5), 16);
    let client = read_all(&mut tee)?;
    assert_eq!(client, PAYLOAD, "the client still receives every byte");

    let outcome = tee.into_inspection();
    assert!(outcome.truncated);
    let Parse::Unparseable(failure) = outcome.parse else {
        panic!("a 16-byte head of a 48-byte document parsed");
    };
    assert_eq!(failure.class, ParseClass::Eof);
    Ok(())
}

#[test]
fn a_complete_capture_parses_without_truncation() -> io::Result<()> {
    const PAYLOAD: &[u8] = br#"{"ok":true}"#;
    let mut tee = Tee::with_cap(ChunkedReader::new(PAYLOAD, 4), 1024);
    read_all(&mut tee)?;

    let outcome = tee.into_inspection();
    assert!(!outcome.truncated);
    assert_eq!(
        outcome.parse,
        Parse::Parsed(Value::Object(vec![("ok".into(), Value::Bool(true)),]))
    );
    Ok(())
}

/// `truncated` describes the *body*, not the parse. A cap that ends just past
/// a closing brace yields a head that parses cleanly while the body it came
/// from did not finish — so a classifier reading `parse` alone would call this
/// a whole, well-formed document.
#[test]
fn a_parsed_head_can_still_come_from_a_truncated_body() -> io::Result<()> {
    const PAYLOAD: &[u8] = br#"{"ok":true} and a tail the cap never sees"#;
    let mut tee = Tee::with_cap(ChunkedReader::new(PAYLOAD, 4), 12);
    let client = read_all(&mut tee)?;
    assert_eq!(client, PAYLOAD, "the client still receives every byte");

    let outcome = tee.into_inspection();
    assert!(outcome.truncated);
    assert_eq!(
        outcome.parse,
        Parse::Parsed(Value::Object(vec![("ok".into(), Value::Bool(true)),]))
    );
    Ok(())
}

/// An upstream that dies mid-body cuts the capture just as the cap does, so the
/// head must be reported as a prefix. Without the flag a cut body is
/// indistinguishable from vendor drift and gets filed as `malformed_body`.
#[test]
fn a_body_cut_by_an_upstream_failure_is_reported_as_a_prefix() -> io::Result<()> {
    let mut tee = Tee::with_cap(FailingReader::after(3), 4096);
    assert!(
        read_all(&mut tee).is_err(),
        "the upstream was supposed to fail"
    );

    let outcome = tee.into_inspection();
    assert!(outcome.truncated, "the body never finished");
    let Parse::Unparseable(failure) = outcome.parse else {
        panic!("'uuu' parsed as JSON");
    };
    assert_ne!(
        failure.class,
        ParseClass::Eof,
        "a bare prefix that is not JSON is a syntax failure, not a cut body"
    );
    Ok(())
}

#[test]
fn serde_json_categories_map_to_stable_classes() {
    // Parser and typed-deserialization errors share this stable vocabulary.
    let Err(error) = serde_json::from_str::<u32>("\"x\"") else {
        panic!("a JSON string parsed as u32");
    };
    assert_eq!(ParseFailure::from(&error).class, ParseClass::Data);

    let Err(error) = serde_json::from_str::<u32>("") else {
        panic!("empty input parsed as u32");
    };
    assert_eq!(ParseFailure::from(&error).class, ParseClass::Eof);

    let mut failing = FailingReader::after(0);
    let Err(error) = serde_json::from_reader::<_, serde_json::Value>(&mut failing) else {
        panic!("a failing reader parsed as JSON");
    };
    assert_eq!(ParseFailure::from(&error).class, ParseClass::Io);
}

/// `ParseFailure` documents I/O failures as carrying no position. That is a
/// promise about a third-party parser, so it is pinned here: a fabricated
/// 1-based position would send a classifier looking at a line of a body that
/// never arrived.
#[test]
fn an_io_failure_carries_no_position() {
    let mut failing = FailingReader::after(0);
    let Err(error) = serde_json::from_reader::<_, serde_json::Value>(&mut failing) else {
        panic!("a failing reader parsed as JSON");
    };

    let failure = ParseFailure::from(&error);
    assert_eq!(failure.class, ParseClass::Io);
    assert_eq!(
        (failure.line, failure.column),
        (0, 0),
        "an I/O error stops nowhere in particular"
    );
    assert!(!failure.message.is_empty(), "the reason is still recorded");
}
