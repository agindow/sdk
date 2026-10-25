#[path = "support/windows.rs"]
mod support;
use agw_stats::windows::{Availability, Error, Store};
use std::sync::Arc;
use support::*;

#[test]
fn sample_and_scope_pressure_roll_back_all_staged_fields() -> Result<(), Box<dyn std::error::Error>>
{
    let mut l = limits();
    l.samples = 2;
    l.minimum = 1;
    l.scopes = 2;
    l.batch = 2;
    let mut store = Store::new(l)?;
    let a = selection(scope());
    let mut b = a.clone();
    b.scope.tenant = "b".into();
    record(&mut store, std::slice::from_ref(&a), br#"{"value":1}"#, 0)?;
    store.publish(&a.scope)?;
    for i in 1..3 {
        record(&mut store, std::slice::from_ref(&a), br#"{"value":1}"#, i)?;
    }
    let before = store.snapshot(&a.scope).ready().ok_or("not ready")?;
    let bytes = store.bytes();
    assert_eq!(
        record(&mut store, &[b.clone(), a.clone()], br#"{"value":2}"#, 3),
        Err(Error::ResourceLimit)
    );
    assert_eq!(store.bytes(), bytes);
    assert_eq!(store.scopes(), 1);
    record(&mut store, std::slice::from_ref(&b), br#"{"value":2}"#, 4)?;
    store.publish(&b.scope)?;
    let mut c = b.clone();
    c.scope.generation = "new".into();
    assert_eq!(
        record(&mut store, &[c], b"{}", 5),
        Err(Error::ResourceLimit)
    );
    assert!(Arc::ptr_eq(
        before.baseline(),
        store
            .snapshot(&a.scope)
            .ready()
            .ok_or("not ready")?
            .baseline()
    ));
    assert_eq!(store.scopes(), 2);
    assert!(store.remove(&a.scope));
    assert!(store.remove(&b.scope));
    assert_eq!(store.bytes(), 0);
    Ok(())
}

#[test]
fn exact_byte_boundary_and_publish_overhead_are_atomic() -> Result<(), Box<dyn std::error::Error>> {
    let a = selection(scope());
    let mut l = limits();
    l.minimum = 1;
    let mut probe = Store::new(l)?;
    record(&mut probe, std::slice::from_ref(&a), br#"{"value":"x"}"#, 0)?;
    l.bytes = probe.bytes();
    let mut store = Store::new(l)?;
    record(&mut store, std::slice::from_ref(&a), br#"{"value":"x"}"#, 0)?;
    assert_eq!(store.publish(&a.scope), Err(Error::ResourceLimit));
    assert!(matches!(
        store.snapshot(&a.scope),
        Availability::Warming {
            baseline: 0,
            live: 1
        }
    ));
    let mut b = a.clone();
    b.scope.tenant = "b".into();
    assert_eq!(
        record(&mut store, &[b], b"{}", 0),
        Err(Error::ResourceLimit)
    );
    assert_eq!(store.bytes(), l.bytes);
    assert_eq!(store.scopes(), 1);
    l.bytes -= 1;
    let mut too_small = Store::new(l)?;
    assert_eq!(
        record(&mut too_small, &[a], br#"{"value":"x"}"#, 0),
        Err(Error::ResourceLimit)
    );
    assert_eq!(too_small.bytes(), 0);
    Ok(())
}

#[test]
fn invalid_second_selection_and_inspection_limits_preserve_previous_samples()
-> Result<(), Box<dyn std::error::Error>> {
    let mut l = limits();
    l.minimum = 1;
    l.inspection_nodes = 2;
    l.inspection_bytes = 6;
    let mut store = Store::new(l)?;
    let a = selection(scope());
    record(&mut store, std::slice::from_ref(&a), br#"{"value":"x"}"#, 0)?;
    store.publish(&a.scope)?;
    record(&mut store, std::slice::from_ref(&a), br#"{"value":"x"}"#, 0)?;
    let before = store.snapshot(&a.scope).ready().ok_or("not ready")?;
    let bytes = store.bytes();
    for body in [br#"{"value":"xx"}"#.as_slice(), br#"{"value":1,"x":1}"#] {
        assert_eq!(
            record(&mut store, std::slice::from_ref(&a), body, 0),
            Err(Error::ResourceLimit)
        );
    }
    let mut b = a.clone();
    b.scope.field = "/bad~".into();
    assert_eq!(
        record(&mut store, &[a.clone(), b], b"{}", 0),
        Err(Error::UnsupportedPath)
    );
    assert_eq!(
        record(&mut store, &[a.clone(), a.clone()], b"{}", 0),
        Err(Error::InvalidInput)
    );
    let after = store.snapshot(&a.scope).ready().ok_or("not ready")?;
    assert!(Arc::ptr_eq(before.live(), after.live()));
    assert_eq!(store.bytes(), bytes);
    Ok(())
}

#[test]
fn unavailable_provider_retains_reference_and_explicit_relearning_is_isolated()
-> Result<(), Box<dyn std::error::Error>> {
    let mut l = limits();
    l.minimum = 1;
    let mut store = Store::new(l)?;
    let a = selection(scope());
    record(
        &mut store,
        std::slice::from_ref(&a),
        br#"{"value":"old"}"#,
        0,
    )?;
    store.publish(&a.scope)?;
    record(
        &mut store,
        std::slice::from_ref(&a),
        br#"{"value":"new"}"#,
        1,
    )?;
    let original = store.snapshot(&a.scope).ready().ok_or("not ready")?;
    store.set_available(&a.scope, false)?;
    for _ in 0..100 {
        assert!(matches!(
            store.snapshot(&a.scope),
            Availability::Unavailable(Error::ProviderUnavailable)
        ));
    }
    assert_eq!(
        record(&mut store, std::slice::from_ref(&a), b"{}", 2),
        Err(Error::ProviderUnavailable)
    );
    assert_eq!(store.publish(&a.scope), Err(Error::ProviderUnavailable));
    store.set_available(&a.scope, true)?;
    assert!(Arc::ptr_eq(
        original.baseline(),
        store
            .snapshot(&a.scope)
            .ready()
            .ok_or("not ready")?
            .baseline()
    ));
    let mut b = a.clone();
    b.scope.generation = "v2".into();
    assert_eq!(
        record(&mut store, std::slice::from_ref(&b), b"{", 3),
        Err(Error::Incomplete)
    );
    record(
        &mut store,
        std::slice::from_ref(&b),
        br#"{"value":"new"}"#,
        3,
    )?;
    store.publish(&b.scope)?;
    record(
        &mut store,
        std::slice::from_ref(&b),
        br#"{"value":"new"}"#,
        4,
    )?;
    assert_eq!(
        store
            .snapshot(&b.scope)
            .ready()
            .ok_or("not ready")?
            .baseline()
            .vocabulary(),
        vec!["new"]
    );
    assert_eq!(original.baseline().vocabulary(), vec!["old"]);
    Ok(())
}

#[test]
fn batch_width_configuration_and_scalar_limits_reject_before_admission()
-> Result<(), Box<dyn std::error::Error>> {
    for invalid in [
        {
            let mut l = limits();
            l.minimum = 0;
            l
        },
        {
            let mut l = limits();
            l.samples = 4097;
            l
        },
        {
            let mut l = limits();
            l.minimum = l.samples + 1;
            l
        },
        {
            let mut l = limits();
            l.batch = l.scopes + 1;
            l
        },
        {
            let mut l = limits();
            l.inspection_nodes = 0;
            l
        },
        {
            let mut l = limits();
            l.inspection_bytes = 0;
            l
        },
    ] {
        assert!(matches!(Store::new(invalid), Err(Error::InvalidInput)));
    }
    let mut l = limits();
    l.batch = 1;
    let mut store = Store::new(l)?;
    let a = selection(scope());
    let mut b = a.clone();
    b.scope.tenant = "b".into();
    assert_eq!(record(&mut store, &[], b"{}", 0), Err(Error::ResourceLimit));
    assert_eq!(
        record(&mut store, &[a.clone(), b], b"{}", 0),
        Err(Error::ResourceLimit)
    );
    let large = format!(r#"{{"value":"{}"}}"#, "x".repeat(4097));
    assert_eq!(
        record(&mut store, std::slice::from_ref(&a), large.as_bytes(), 0),
        Err(Error::ResourceLimit)
    );
    let large_number = format!(r#"{{"value":1.{}}}"#, "0".repeat(4096));
    assert_eq!(
        record(
            &mut store,
            std::slice::from_ref(&a),
            large_number.as_bytes(),
            0
        ),
        Err(Error::ResourceLimit)
    );
    assert_eq!(store.scopes(), 0);
    assert_eq!(store.bytes(), 0);
    let mut invalid_key = a.scope.clone();
    invalid_key.tenant = "x".repeat(4097);
    assert!(matches!(
        store.snapshot(&invalid_key),
        Availability::Unavailable(Error::InvalidInput)
    ));
    Ok(())
}

#[test]
fn byte_pressure_in_later_selection_leaves_no_staged_scope()
-> Result<(), Box<dyn std::error::Error>> {
    let a = selection(scope());
    let mut b = a.clone();
    b.scope.tenant = "b".into();
    let mut probe = Store::new(limits())?;
    record(&mut probe, std::slice::from_ref(&a), b"{}", 0)?;
    let mut l = limits();
    l.bytes = probe.bytes();
    let mut store = Store::new(l)?;
    assert_eq!(
        record(&mut store, &[a, b], b"{}", 0),
        Err(Error::ResourceLimit)
    );
    assert_eq!(store.scopes(), 0);
    assert_eq!(store.bytes(), 0);
    Ok(())
}
