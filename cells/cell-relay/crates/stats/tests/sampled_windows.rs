#[path = "support/windows.rs"]
mod support;
use agw_inspector::{Precision, Value, inspect};
use agw_stats::windows::{
    self,
    sampled::{self, Draw, Policy, Selection, Store},
};
use chrono::{TimeZone, Utc};
use std::{error::Error, sync::Arc};

fn selection() -> Selection {
    Selection {
        scope: support::scope(),
        array: "/rows".into(),
        item: "/value".into(),
        policy: Policy {
            version: "v1".into(),
            max_items: 1000,
        },
        projection: windows::Projection::ExactInteger(Precision::Observed),
    }
}
fn store() -> Result<Store, windows::Error> {
    let mut limits = support::limits();
    limits.inspection_nodes = 5000;
    Store::new(limits)
}
fn record(
    store: &mut Store,
    s: &Selection,
    body: &[u8],
    entropy: u64,
) -> Result<(), sampled::Error> {
    store.record(
        &[Draw {
            selection: s,
            entropy,
        }],
        &inspect(body, false, None),
        Utc.timestamp_opt(42, 0)
            .single()
            .ok_or(sampled::Error::InvalidPolicy)?,
    )
}
fn ready(body: &[u8]) -> Result<(Store, Selection), Box<dyn Error>> {
    let (mut store, s) = (store()?, selection());
    for _ in 0..20 {
        record(&mut store, &s, body, 1000)?;
    }
    store.publish(&s)?;
    for _ in 0..20 {
        record(&mut store, &s, body, 1000)?;
    }
    Ok((store, s))
}
#[test]
fn thousand_items_are_one_call_opportunity() -> Result<(), Box<dyn Error>> {
    let (mut store, s) = (store()?, selection());
    let body = format!("{{\"rows\":[{}]}}", vec!["{\"value\":1}"; 1000].join(","));
    record(&mut store, &s, body.as_bytes(), 1000)?;
    assert!(matches!(
        store.snapshot(&s),
        sampled::Availability::Warming {
            baseline: 0,
            live: 1
        }
    ));
    assert!(store.publish(&s).is_err());
    for _ in 1..20 {
        record(&mut store, &s, body.as_bytes(), 1000)?;
    }
    store.publish(&s)?;
    assert!(matches!(
        store.snapshot(&s),
        sampled::Availability::Warming {
            baseline: 20,
            live: 0
        }
    ));
    Ok(())
}
#[test]
fn controlled_entropy_reaches_every_index_and_replays() -> Result<(), Box<dyn Error>> {
    for index in 0..7 {
        let body = br#"{"rows":[{"value":0},{"value":1},{"value":2},{"value":3},{"value":4},{"value":5},{"value":6}]}"#;
        let (mut store, s) = (store()?, selection());
        for _ in 0..20 {
            record(&mut store, &s, body, 7 + index)?;
        }
        store.publish(&s)?;
        for _ in 0..20 {
            record(&mut store, &s, body, 7 + index)?;
        }
        let snapshot = store.snapshot(&s).ready().ok_or("not ready")?;
        assert_eq!(
            snapshot.live().samples()[0].value(),
            Some(&Value::Number(agw_inspector::Number::I64(index as i64)))
        );
    }
    let (mut store, s) = (store()?, selection());
    assert!(matches!(
        record(&mut store, &s, br#"{"rows":[{},{},{}]}"#, 0),
        Err(sampled::Error::EntropyRejected)
    ));
    assert_eq!(store.scopes(), 0);
    Ok(())
}
#[test]
fn no_array_opportunity_never_becomes_missing_field() -> Result<(), Box<dyn Error>> {
    let (mut store, s) = (store()?, selection());
    for body in [br#"{}"#.as_slice(), br#"{"rows":null}"#, br#"{"rows":[]}"#] {
        assert!(matches!(
            record(&mut store, &s, body, 1000),
            Err(sampled::Error::NoOpportunity)
        ));
        assert_eq!(store.scopes(), 0);
    }
    for body in [
        br#"{"rows":[{}]}"#.as_slice(),
        br#"{"rows":[{"value":null}]}"#,
    ] {
        let (store, _) = ready(body)?;
        let snapshot = store.snapshot(&s).ready().ok_or("not ready")?;
        assert_eq!(snapshot.live().samples().len(), 20);
        assert_eq!(
            snapshot.live().samples()[0].value().is_none(),
            body == br#"{"rows":[{}]}"#
        );
    }
    Ok(())
}
#[test]
fn unsupported_items_and_leaves_reject_atomically() -> Result<(), Box<dyn Error>> {
    for body in [
        br#"{"rows":[{},null]}"#.as_slice(),
        br#"{"rows":[{},1]}"#,
        br#"{"rows":[{},[]]}"#,
        br#"{"rows":[{"value":{}}]}"#,
        br#"{"rows":[{"value":[]}]}"#,
        br#"{"rows":{}}"#,
        br#"{"rows":[{}, {"x":1,"x":2}]}"#,
    ] {
        let (mut store, s) = (store()?, selection());
        assert!(record(&mut store, &s, body, 1000).is_err());
        assert_eq!(store.scopes(), 0);
        assert_eq!(store.bytes(), 0);
    }
    Ok(())
}
#[test]
fn exact_values_precision_and_timestamps_survive() -> Result<(), Box<dyn Error>> {
    let (mut store, mut s) = (store()?, selection());
    s.projection = windows::Projection::ExactInteger(Precision::HighPrecision);
    for _ in 0..20 {
        record(&mut store, &s, br#"{"rows":[{"value":1e400}]}"#, 0)?;
    }
    store.publish(&s)?;
    for _ in 0..20 {
        record(
            &mut store,
            &s,
            br#"{"rows":[{"value":" a\u0000\u00e9 "}]}"#,
            0,
        )?;
    }
    let snapshot = store.snapshot(&s).ready().ok_or("not ready")?;
    assert_eq!(
        serde_json::to_string(&snapshot.baseline().samples()[0].value())?,
        "1e400"
    );
    assert_eq!(
        snapshot.live().samples()[0].value(),
        Some(&Value::String(" a\0é ".into()))
    );
    assert_eq!(snapshot.live().samples()[0].at().timestamp(), 42);
    assert!(snapshot.baseline().unit().is_none());
    assert_eq!(snapshot.unit(), sampled::SamplingUnit::EligibleCall);
    Ok(())
}
#[test]
fn scopes_policies_and_snapshots_remain_isolated() -> Result<(), Box<dyn Error>> {
    let (mut store, s) = ready(br#"{"rows":[{"value":"old"}]}"#)?;
    let old = store.snapshot(&s).ready().ok_or("not ready")?;
    for mut other in [s.clone(), s.clone(), s.clone()] {
        if store.scopes() == 1 {
            other.policy.version = "v2".into();
        } else if store.scopes() == 2 {
            other.scope.tenant = "other".into();
        } else {
            other.policy.max_items = 999;
        }
        record(&mut store, &other, br#"{"rows":[{"value":"new"}]}"#, 0)?;
        assert!(matches!(
            store.snapshot(&other),
            sampled::Availability::Warming {
                baseline: 0,
                live: 1
            }
        ));
    }
    store.advance(&s)?;
    for _ in 0..20 {
        record(&mut store, &s, br#"{"rows":[{"value":"new"}]}"#, 0)?;
    }
    let new = store.snapshot(&s).ready().ok_or("not ready")?;
    assert!(Arc::ptr_eq(old.baseline(), new.baseline()));
    assert_eq!(new.baseline().vocabulary(), vec!["old"]);
    assert_eq!(old.live().vocabulary(), vec!["old"]);
    assert_eq!(new.live().vocabulary(), vec!["new"]);
    Ok(())
}
#[test]
fn object_pointers_keep_literal_star_numeric_and_empty_names() -> Result<(), Box<dyn Error>> {
    let mut plain = windows::Store::new(support::limits())?;
    let mut scope = support::scope();
    scope.field = "/rows/0/value".into();
    let selected = support::selection(scope.clone());
    assert_eq!(
        support::record(&mut plain, &[selected], br#"{"rows":[{"value":1}]}"#, 42),
        Err(windows::Error::UnsupportedPath)
    );
    scope.field = "/rows/*/0/".into();
    support::record(
        &mut plain,
        &[support::selection(scope)],
        br#"{"rows":{"*":{"0":{"":1}}}}"#,
        42,
    )?;
    let (mut store, mut s) = (store()?, selection());
    s.array = "/a~1b/~0/".into();
    s.item = "/~0~1/".into();
    for _ in 0..20 {
        record(
            &mut store,
            &s,
            br#"{"a/b":{"~":{"":[{"~/":{"":"exact"}}]}}}"#,
            0,
        )?;
    }
    store.publish(&s)?;
    for _ in 0..20 {
        record(
            &mut store,
            &s,
            br#"{"a/b":{"~":{"":[{"~/":{"":"exact"}}]}}}"#,
            0,
        )?;
    }
    let snapshot = store.snapshot(&s).ready().ok_or("not ready")?;
    assert_eq!(snapshot.live().vocabulary(), vec!["exact"]);
    Ok(())
}
