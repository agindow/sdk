#[path = "support/idempotency.rs"]
mod support;
use agw_guard::{
    idempotency::{Bindings, DurabilityStatus, Limits, Request, Store},
    proxy::Headers,
};
use agw_policy::Operation;
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use support::{completed, request, store};
type Result = std::result::Result<(), Box<dyn std::error::Error>>;

#[test]
fn writes_and_unknown_require_valid_keys_before_execution() -> Result {
    let cache = store::<()>(4, Duration::from_secs(60))?;
    let calls = AtomicUsize::new(0);
    for operation in [Operation::Write, Operation::Unknown] {
        let oversized = "x".repeat(257);
        for key in [
            None,
            Some(""),
            Some(" "),
            Some("key\r\nspoof"),
            Some(oversized.as_str()),
        ] {
            let error = cache
                .execute(
                    Request {
                        operation,
                        ..request("conn", key)
                    },
                    |_| {
                        calls.fetch_add(1, Ordering::SeqCst);
                    },
                )
                .err()
                .ok_or("missing rejection")?;
            assert_eq!(error.http_status(), 400);
        }
    }
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    Ok(())
}

#[test]
fn replay_shares_the_exact_original_value_and_error() -> Result {
    let cache = store(4, Duration::from_secs(60))?;
    let original = Box::new(String::from("original vendor failure"));
    let ptr = &*original as *const String;
    let first =
        completed(cache.execute(request("conn", Some("key")), |_| Err::<(), _>(original))?)?;
    let replay =
        completed(cache.execute(request("conn", Some("key")), |_| panic!("must replay"))?)?;
    assert!(!first.replayed && replay.replayed);
    assert!(Arc::ptr_eq(&first.value, &replay.value));
    assert_eq!(
        first
            .value
            .as_ref()
            .as_ref()
            .err()
            .map(|v| &**v as *const String),
        Some(ptr)
    );
    Ok(())
}

#[test]
fn connections_and_tenant_stores_are_isolated_and_reads_never_deduplicate() -> Result {
    let first = store(4, Duration::from_secs(60))?;
    let other = Store::new(
        "other-tenant",
        Limits::default(),
        Bindings::new("Idempotency-Key", None)?,
    )?;
    assert_eq!(other.tenant(), "other-tenant");
    for (cache, connection, value) in [(&first, "one", 1), (&first, "two", 2), (&other, "one", 3)] {
        assert_eq!(
            *completed(cache.execute(request(connection, Some("key")), |_| value)?)?.value,
            value
        );
    }
    for value in [4, 5] {
        let result = completed(first.execute(
            Request {
                operation: Operation::Read,
                ..request("one", None)
            },
            |_| value,
        )?)?;
        assert_eq!(*result.value, value);
        assert!(!result.replayed);
    }
    Ok(())
}

#[test]
fn configured_capacity_preserves_unexpired_records_but_expiry_allows_new_execution() -> Result {
    let cache = store(1, Duration::from_secs(60))?;
    let first = completed(cache.execute(request("one", Some("key")), |_| 7)?)?;
    let error = cache
        .execute(request("two", Some("new")), |_| 9)
        .err()
        .ok_or("capacity rejection")?;
    assert_eq!(error.http_status(), 429);
    let replay = completed(cache.execute(request("one", Some("key")), |_| 10)?)?;
    assert!(Arc::ptr_eq(&first.value, &replay.value));
    let cache = store(1, Duration::from_millis(5))?;
    completed(cache.execute(request("one", Some("key")), |_| 7)?)?;
    std::thread::sleep(Duration::from_millis(10));
    let expired = completed(cache.execute(request("one", Some("key")), |_| 11)?)?;
    assert!(!expired.replayed);
    assert_eq!(*expired.value, 11);
    Ok(())
}

#[test]
fn key_and_supported_durability_are_claimed_verbatim_on_every_attempt() -> Result {
    let cache = Store::new(
        "tenant",
        Limits::default(),
        Bindings::new("Idempotency-Key", Some("Vendor-Durability"))?,
    )?;
    let result = completed(cache.execute(
        Request {
            durability: Some("committed"),
            ..request("conn", Some("Exact-Key_01"))
        },
        |forwarding| {
            for _ in 0..2 {
                let mut headers = Headers::from(vec![
                    ("idempotency-key".into(), "wrong".into()),
                    ("IDEMPOTENCY-KEY".into(), "also-wrong".into()),
                    ("vendor-durability".into(), "weak".into()),
                    ("Other".into(), "untouched".into()),
                ]);
                forwarding.apply_headers(&mut headers);
                assert_eq!(headers.get("Idempotency-Key"), Some("Exact-Key_01"));
                assert_eq!(headers.get("Vendor-Durability"), Some("committed"));
                assert_eq!(headers.get("Other"), Some("untouched"));
                assert_eq!(
                    headers
                        .iter()
                        .filter(|(name, _)| name.eq_ignore_ascii_case("Idempotency-Key"))
                        .count(),
                    1
                );
            }
            assert_eq!(forwarding.key, Some("Exact-Key_01"));
        },
    )?)?;
    assert_eq!(result.durability, DurabilityStatus::Forwarded);
    Ok(())
}

#[test]
fn unsupported_durability_is_explicit_and_invalid_configuration_is_rejected() -> Result {
    let cache = store(1, Duration::from_secs(60))?;
    let result = completed(cache.execute(
        Request {
            durability: Some("sync"),
            ..request("conn", Some("key"))
        },
        |forwarding| {
            assert_eq!(forwarding.durability, None);
        },
    )?)?;
    assert_eq!(result.durability, DurabilityStatus::Unsupported);
    assert!(result.durability.warning().is_some());
    for (key, durability) in [("bad\r\nheader", None), ("Key", Some("key")), ("", None)] {
        assert!(Bindings::new(key, durability).is_err());
    }
    assert!(store::<()>(0, Duration::from_secs(60)).is_err());
    assert!(store::<()>(1, Duration::ZERO).is_err());
    assert!(store::<()>(1, Duration::from_secs(8 * 86400)).is_err());
    Ok(())
}
