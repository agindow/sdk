#[path = "support/windows.rs"]
mod support;
use agw_stats::{
    enumeration, field, unit,
    windows::{Availability, Error, Store},
};
use std::sync::Arc;
use support::*;

#[test]
fn frozen_reference_never_learns_live_drift_or_current_call()
-> Result<(), Box<dyn std::error::Error>> {
    let mut store = Store::new(limits())?;
    let selected = selection(scope());
    assert!(matches!(
        store.snapshot(&selected.scope),
        Availability::Unavailable(_)
    ));
    record(
        &mut store,
        std::slice::from_ref(&selected),
        br#"{"value":"old"}"#,
        0,
    )?;
    assert!(matches!(
        store.snapshot(&selected.scope),
        Availability::Warming {
            baseline: 0,
            live: 1
        }
    ));
    assert_eq!(
        store.publish(&selected.scope),
        Err(Error::InsufficientSamples)
    );
    for i in 1..20 {
        record(
            &mut store,
            std::slice::from_ref(&selected),
            br#"{"value":"old"}"#,
            i,
        )?;
    }
    store.publish(&selected.scope)?;
    assert!(matches!(
        store.snapshot(&selected.scope),
        Availability::Warming {
            baseline: 20,
            live: 0
        }
    ));
    for i in 20..40 {
        record(
            &mut store,
            std::slice::from_ref(&selected),
            br#"{"value":"new"}"#,
            i,
        )?;
    }
    let before = store.snapshot(&selected.scope).ready().ok_or("not ready")?;
    assert_eq!(before.baseline().vocabulary(), vec!["old"]);
    let report = enumeration::compare(
        "/value",
        &before.baseline().vocabulary(),
        &before.live().enumeration(),
    )?;
    assert_eq!(report.new_values()[0].first_seen.timestamp(), 20);
    assert_eq!(store.publish(&selected.scope), Err(Error::AlreadyPublished));
    record(
        &mut store,
        std::slice::from_ref(&selected),
        br#"{"value":"later"}"#,
        40,
    )?;
    let after = store.snapshot(&selected.scope).ready().ok_or("not ready")?;
    assert!(Arc::ptr_eq(before.baseline(), after.baseline()));
    assert_eq!(before.live().samples().len(), 20);
    assert_eq!(after.live().samples().len(), 21);
    store.advance(&selected.scope)?;
    assert!(matches!(
        store.snapshot(&selected.scope),
        Availability::Warming {
            baseline: 20,
            live: 0
        }
    ));
    assert_eq!(before.live().samples().len(), 20);
    Ok(())
}

#[test]
fn every_scope_dimension_isolates_windows_and_readiness() -> Result<(), Box<dyn std::error::Error>>
{
    let original = scope();
    let mut scopes = vec![original.clone()];
    for index in 0..9 {
        let mut other = original.clone();
        match index {
            0 => other.tenant = "b".into(),
            1 => other.connection = "b".into(),
            2 => other.vendor = "b".into(),
            3 => other.operation = "b".into(),
            4 => other.side = agw_diff::Side::Request,
            5 => other.field = "/other".into(),
            6 => other.segment = "b".into(),
            7 => other.generation = "v2".into(),
            _ => other.window = "hour-2".into(),
        }
        scopes.push(other);
    }
    let mut store = Store::new(limits())?;
    for i in 0..20 {
        for (j, scope) in scopes.iter().enumerate() {
            let body = format!(r#"{{"value":"{j}","other":"{j}"}}"#);
            record(&mut store, &[selection(scope.clone())], body.as_bytes(), i)?;
        }
    }
    for (j, scope) in scopes.iter().enumerate() {
        store.publish(scope)?;
        assert!(matches!(
            store.snapshot(scope),
            Availability::Warming {
                baseline: 20,
                live: 0
            }
        ));
        for i in 20..40 {
            record(
                &mut store,
                &[selection(scope.clone())],
                br#"{"value":0,"other":0}"#,
                i,
            )?;
        }
        let snapshot = store.snapshot(scope).ready().ok_or("not ready")?;
        assert_eq!(
            snapshot.baseline().vocabulary(),
            vec![j.to_string().as_str()]
        );
    }
    Ok(())
}

#[test]
fn extracted_windows_drive_unit_and_field_detectors() -> Result<(), Box<dyn std::error::Error>> {
    let mut store = Store::new(limits())?;
    let selected = selection(scope());
    for i in 0..20 {
        record(
            &mut store,
            std::slice::from_ref(&selected),
            br#"{"value":100}"#,
            i,
        )?;
    }
    store.publish(&selected.scope)?;
    for i in 20..40 {
        record(
            &mut store,
            std::slice::from_ref(&selected),
            br#"{"value":10000}"#,
            i,
        )?;
    }
    let s = store.snapshot(&selected.scope).ready().ok_or("not ready")?;
    assert!(matches!(
        unit::detect(
            "/value",
            &s.baseline().unit().ok_or("unit unavailable")?,
            &s.live().unit().ok_or("unit unavailable")?
        )?,
        unit::Detection::Drift(_)
    ));
    assert_eq!(field::profile(&s.live().fields())?.opportunities(), 20);
    store.advance(&selected.scope)?;
    for i in 40..60 {
        record(&mut store, std::slice::from_ref(&selected), b"{}", i)?;
    }
    let s = store.snapshot(&selected.scope).ready().ok_or("not ready")?;
    assert!(
        !field::completeness("/value", &s.baseline().fields(), &s.live().fields())?
            .reasons()
            .is_empty()
    );
    Ok(())
}

#[test]
fn materialized_field_windows_replay_type_and_format_changes()
-> Result<(), Box<dyn std::error::Error>> {
    for (baseline, observed, format) in [
        (
            br#"{"value":1}"#.as_slice(),
            br#"{"value":"1"}"#.as_slice(),
            false,
        ),
        (
            br#"{"value":"2026-01-05"}"#.as_slice(),
            br#"{"value":"Jan 5"}"#.as_slice(),
            true,
        ),
    ] {
        let mut store = Store::new(limits())?;
        let selected = selection(scope());
        for i in 0..20 {
            record(&mut store, std::slice::from_ref(&selected), baseline, i)?;
        }
        store.publish(&selected.scope)?;
        for i in 20..40 {
            record(&mut store, std::slice::from_ref(&selected), observed, i)?;
        }
        let snapshot = store.snapshot(&selected.scope).ready().ok_or("not ready")?;
        let (a, b) = (snapshot.baseline().fields(), snapshot.live().fields());
        let report = if format {
            field::charset("/value", &a, &b)?
        } else {
            field::coercion("/value", &a, &b)?
        };
        assert!(!report.reasons().is_empty());
    }
    Ok(())
}
