#[path = "support/windows.rs"]
mod support;
use agw_inspector::{FieldPrecisions, Number, Parse, Precision, Value, inspect, inspect_fields};
use agw_stats::{
    field::{self, Sentinel},
    windows::{Error, Projection, Store},
};
use chrono::Utc;
use support::*;

#[test]
fn missing_null_zero_empty_and_escaped_keys_remain_distinct()
-> Result<(), Box<dyn std::error::Error>> {
    let mut l = limits();
    l.minimum = 1;
    let mut store = Store::new(l)?;
    let mut selected = selection(scope());
    selected.scope.field = "/a~1b/~0/".into();
    let bodies: [&[u8]; 5] = [
        b"{}",
        br#"{"a/b":{"~":{"":null}}}"#,
        br#"{"a/b":{"~":{"":-0e400}}}"#,
        br#"{"a/b":{"~":{"":""}}}"#,
        br#"{"a/b":{"~":{"":" A\u0042 "}}}"#,
    ];
    record(&mut store, std::slice::from_ref(&selected), bodies[4], 0)?;
    store.publish(&selected.scope)?;
    for (i, body) in bodies.iter().enumerate() {
        record(&mut store, std::slice::from_ref(&selected), body, i as i64)?;
    }
    let snapshot = store.snapshot(&selected.scope).ready().ok_or("not ready")?;
    let values = snapshot.live().fields();
    assert_eq!(values[0], None);
    assert_eq!(values[1], Some(&Value::Null));
    assert!(matches!(values[2], Some(Value::Number(n)) if n.lexeme() == "-0e400"));
    assert_eq!(values[3], Some(&Value::String(String::new())));
    assert_eq!(snapshot.live().vocabulary(), vec!["", " AB "]);
    let profile = field::profile(&values)?;
    for sentinel in [
        Sentinel::Missing,
        Sentinel::Null,
        Sentinel::Zero,
        Sentinel::EmptyString,
    ] {
        assert_eq!(profile.sentinel_count(sentinel), 1);
    }
    assert_eq!(profile.opportunities(), 5);
    Ok(())
}

#[test]
fn ambiguous_unsupported_and_incomplete_batches_admit_nothing()
-> Result<(), Box<dyn std::error::Error>> {
    let mut store = Store::new(limits())?;
    let selected = selection(scope());
    for body in [
        br#"{"value":1,"unselected":{"x":1,"x":2}}"#.as_slice(),
        br#"{"value":1,"x":[{"a":1,"a":2}]}"#,
    ] {
        assert_eq!(
            record(&mut store, std::slice::from_ref(&selected), body, 0),
            Err(Error::DuplicateKey)
        );
    }
    for path in ["/items/0", "/value", "/absent/~2", "$.value", ""] {
        let mut bad = selected.clone();
        bad.scope.field = path.into();
        assert!(record(&mut store, &[bad], br#"{"items":[1],"value":{}}"#, 0).is_err());
    }
    let mut scalar_parent = selected.clone();
    scalar_parent.scope.field = "/value/x".into();
    assert_eq!(
        record(&mut store, &[scalar_parent], br#"{"value":1}"#, 0),
        Err(Error::UnsupportedPath)
    );
    for (body, cut) in [(b"{}".as_slice(), true), (b"{", false), (b"{", true)] {
        assert_eq!(
            store.record(
                std::slice::from_ref(&selected),
                &inspect(body, cut, None),
                Utc::now()
            ),
            Err(Error::Incomplete)
        );
    }
    let mut synthetic = inspect(b"{}", false, None);
    synthetic.parse = Parse::Parsed(Value::Object(vec![(
        "value".into(),
        Value::Number(Number::Verbatim("NaN".into())),
    )]));
    assert_eq!(
        store.record(&[selected], &synthetic, Utc::now()),
        Err(Error::InvalidInput)
    );
    assert_eq!(store.scopes(), 0);
    assert_eq!(store.bytes(), 0);
    Ok(())
}

#[test]
fn unsafe_numbers_skip_unit_without_losing_type_or_enum_evidence()
-> Result<(), Box<dyn std::error::Error>> {
    for token in [
        "9007199254740992",
        "18446744073709551616",
        "1e400",
        "0.1",
        "-9007199254740992",
    ] {
        let mut l = limits();
        l.minimum = 1;
        let mut store = Store::new(l)?;
        let selected = selection(scope());
        record(
            &mut store,
            std::slice::from_ref(&selected),
            br#"{"value":1}"#,
            0,
        )?;
        store.publish(&selected.scope)?;
        record(
            &mut store,
            std::slice::from_ref(&selected),
            format!(r#"{{"value":{token}}}"#).as_bytes(),
            1,
        )?;
        record(
            &mut store,
            std::slice::from_ref(&selected),
            br#"{"value":"new"}"#,
            2,
        )?;
        let snapshot = store.snapshot(&selected.scope).ready().ok_or("not ready")?;
        assert_eq!(snapshot.live().unit(), None);
        assert_eq!(snapshot.live().vocabulary(), vec!["new"]);
        assert_eq!(field::profile(&snapshot.live().fields())?.non_null(), 2);
        assert!(
            matches!(snapshot.live().samples()[0].value(), Some(Value::Number(n)) if n.lexeme() == token)
        );
    }
    Ok(())
}

#[test]
fn precision_hints_and_authorization_gate_even_small_integer_projection()
-> Result<(), Box<dyn std::error::Error>> {
    for projection in [
        Projection::Disabled,
        Projection::ExactInteger(Precision::HighPrecision),
        Projection::ExactInteger(Precision::Observed),
    ] {
        let mut l = limits();
        l.minimum = 1;
        let mut store = Store::new(l)?;
        let mut selected = selection(scope());
        selected.projection = projection;
        let fields = FieldPrecisions::new([(
            "value",
            Some(projection == Projection::ExactInteger(Precision::HighPrecision)),
        )])?;
        let inspection = inspect_fields(br#"{"value":42}"#, false, &fields);
        store.record(std::slice::from_ref(&selected), &inspection, Utc::now())?;
        store.publish(&selected.scope)?;
        store.record(std::slice::from_ref(&selected), &inspection, Utc::now())?;
        let snapshot = store.snapshot(&selected.scope).ready().ok_or("not ready")?;
        assert_eq!(
            snapshot.live().unit(),
            (projection == Projection::ExactInteger(Precision::Observed)).then_some(vec![42.])
        );
    }
    Ok(())
}
