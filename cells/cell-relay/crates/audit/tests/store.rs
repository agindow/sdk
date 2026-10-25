//! TI-29 local store: append-only, tenant-bound, bounded and explicitly retained.
mod support;

use agw_audit::Error;
use agw_audit::store::{Append, Limits, Store};
use chrono::{Duration, TimeZone, Utc};
use std::error::Error as StdError;

type Result = std::result::Result<(), Box<dyn StdError>>;

fn now() -> chrono::DateTime<Utc> {
    const SECONDS: i64 = 1_761_115_211;
    Utc.timestamp_opt(SECONDS, 0)
        .single()
        .unwrap_or_else(Utc::now)
}

#[test]
fn appending_stores_an_immutable_retrievable_record() -> Result {
    let artifact = support::fixture()?;
    let mut store = Store::new("acme", Limits::default())?;
    assert_eq!(store.append(artifact.clone(), now())?, Append::Stored);
    assert_eq!(store.len(), 1);
    assert!(store.bytes() > 0);
    let stored = store
        .get("aud_01HQ8Z3K4M5N6P7Q8R9S0T1V2W", now())
        .ok_or("missing")?;
    assert_eq!(
        serde_json::to_value(stored)?,
        serde_json::to_value(&artifact)?
    );
    Ok(())
}

#[test]
fn an_identical_retry_is_idempotent_and_a_different_body_conflicts() -> Result {
    let artifact = support::fixture()?;
    let mut store = Store::new("acme", Limits::default())?;
    store.append(artifact.clone(), now())?;
    let bytes = store.bytes();
    assert_eq!(store.append(artifact.clone(), now())?, Append::Duplicate);
    assert_eq!(store.bytes(), bytes);
    assert_eq!(store.len(), 1);

    let mut changed = artifact;
    changed.detail = Some("mutated".try_into()?);
    assert!(matches!(store.append(changed, now()), Err(Error::Conflict)));
    assert_eq!(store.len(), 1);
    Ok(())
}

#[test]
fn dishonest_artifacts_are_never_stored() -> Result {
    let mut artifact = support::fixture()?;
    artifact
        .patch
        .as_mut()
        .ok_or("missing patch")?
        .byte_identical = true;
    let mut store = Store::new("acme", Limits::default())?;
    assert!(matches!(
        store.append(artifact, now()),
        Err(Error::Invalid(_))
    ));
    assert_eq!(store.len(), 0);
    Ok(())
}

#[test]
fn tenant_metadata_must_match_the_store_tenant() -> Result {
    let mut store = Store::new("other", Limits::default())?;
    assert!(matches!(
        store.append(support::fixture()?, now()),
        Err(Error::Invalid(_))
    ));

    let mut artifact = support::fixture()?;
    artifact.metadata.remove("tenant");
    store.append(artifact, now())?;
    Ok(())
}

#[test]
fn record_and_byte_capacity_are_enforced_without_eviction() -> Result {
    let artifact = support::fixture()?;
    let limits = Limits {
        capacity: 1,
        ..Limits::default()
    };
    let mut store = Store::new("acme", limits)?;
    store.append(artifact.clone(), now())?;
    let mut second = artifact.clone();
    second.id = "aud_second".try_into()?;
    assert!(matches!(store.append(second, now()), Err(Error::Capacity)));

    let mut limited = Store::new(
        "acme",
        Limits {
            capacity: 8,
            byte_limit: 16,
            ..Limits::default()
        },
    )?;
    assert!(matches!(
        limited.append(artifact, now()),
        Err(Error::ByteLimit)
    ));
    Ok(())
}

#[test]
fn retention_rejects_stale_publications_and_hides_expired_records() -> Result {
    let mut artifact = support::fixture()?;
    artifact.ts = now() - Duration::minutes(30);
    let limits = Limits {
        retention: Duration::hours(1),
        ..Limits::default()
    };
    let mut store = Store::new("acme", limits)?;
    store.append(artifact, now())?;
    assert!(store.get("aud_01HQ8Z3K4M5N6P7Q8R9S0T1V2W", now()).is_some());
    assert!(
        store
            .get(
                "aud_01HQ8Z3K4M5N6P7Q8R9S0T1V2W",
                now() + Duration::minutes(31)
            )
            .is_none()
    );

    let mut stale = support::fixture()?;
    stale.ts = now() - Duration::hours(2);
    assert!(matches!(store.append(stale, now()), Err(Error::Expired)));
    Ok(())
}

#[test]
fn purge_removes_only_expired_records_and_exports_append_order() -> Result {
    let mut store = Store::new(
        "acme",
        Limits {
            retention: Duration::hours(1),
            ..Limits::default()
        },
    )?;
    let mut old = support::fixture()?;
    old.ts = now() - Duration::minutes(30);
    store.append(old, now())?;
    let mut fresh = support::fixture()?;
    fresh.id = "aud_fresh".try_into()?;
    store.append(fresh, now())?;

    let ids: Vec<_> = store.export_all().map(|a| String::clone(&a.id)).collect();
    assert_eq!(ids, vec!["aud_01HQ8Z3K4M5N6P7Q8R9S0T1V2W", "aud_fresh"]);

    let after = now() + Duration::minutes(31);
    assert_eq!(store.purge_expired(after), 1);
    assert_eq!(store.len(), 1);
    assert_eq!(store.export_all().count(), 1);
    assert_eq!(store.purge_expired(after), 0);
    Ok(())
}

#[test]
fn tenant_and_limit_bounds_are_validated() {
    assert!(matches!(
        Store::new("", Limits::default()),
        Err(Error::Invalid(_))
    ));
    assert!(matches!(
        Store::new(&"x".repeat(257), Limits::default()),
        Err(Error::Invalid(_))
    ));
    assert!(matches!(
        Store::new("bad\u{7f}", Limits::default()),
        Err(Error::Invalid(_))
    ));
    for limits in [
        Limits {
            capacity: 0,
            ..Limits::default()
        },
        Limits {
            capacity: 1_000_001,
            ..Limits::default()
        },
        Limits {
            byte_limit: 0,
            ..Limits::default()
        },
        Limits {
            byte_limit: 512 * 1024 * 1024 + 1,
            ..Limits::default()
        },
        Limits {
            retention: Duration::minutes(30),
            ..Limits::default()
        },
        Limits {
            retention: Duration::days(3651),
            ..Limits::default()
        },
    ] {
        assert!(matches!(Store::new("acme", limits), Err(Error::Invalid(_))));
    }
}
