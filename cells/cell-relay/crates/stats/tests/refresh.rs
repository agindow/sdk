use agw_stats::refresh::{Cache, Error, Limits, Selected};
use serde_json::{Value, json};
use std::sync::Arc;
fn selected(generation: &str) -> Result<Selected, Box<dyn std::error::Error>> {
    Ok(serde_json::from_value(
        json!({"scope":{"tenant":"tenant-a","vendor":"payments",
        "operation":"GET /charges","side":"response","field":"/amount","volume_cohort":"small",
        "tenant_segment":serde_json::to_string(&json!(["ti48","conn",generation,"retail","hour"]))?},
        "generation":generation,"grain":"hour","bucket":"2026-01-01T00:00:00.000Z",
        "start":"2026-01-01T00:00:00.000Z","end":"2026-01-01T01:00:00.000Z"}),
    )?)
}
fn body(s: &Selected, revision: u64) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut v: Value = serde_json::from_str(include_str!(
        "../../../../../contracts/examples/profile_snapshot.valid.json"
    ))?;
    v["generation"] = s.generation.clone().into();
    v["profile"]["scope"] = serde_json::to_value(&s.scope)?;
    v["profile"]["start"] = s.start.clone().into();
    v["profile"]["end"] = s.end.clone().into();
    v["bucket"] = s.bucket.clone().into();
    v["revision"] = revision.into();
    Ok(serde_json::to_vec(&v)?)
}
fn limits() -> Limits {
    Limits {
        capacity: 2,
        bytes: 262144,
        ttl_ms: 100,
        refresh_ms: 40,
        timeout_ms: 10,
        backoff_ms: 5,
        recovery_ms: 50,
        attempts: 2,
    }
}
#[test]
fn startup_recovery_expiry_and_immutable_calls() -> Result<(), Box<dyn std::error::Error>> {
    let mut cache = Cache::new(limits())?;
    let s = selected("g1")?;
    let id = cache.select(s.clone(), 0)?;
    assert!(cache.read(id, 0).is_none());
    let job = cache.begin(0).ok_or("startup refresh missing")?;
    assert!(cache.begin(0).is_none());
    cache.complete(job, Ok(body(&s, 1)?), 1)?;
    let old = cache.read(id, 1).ok_or("cold")?;
    assert_eq!(old.revision(), 1);
    assert!(old.exact_detector_inputs().is_none());
    assert!(cache.begin(40).is_none());
    let job = cache.begin(41).ok_or("scheduled recovery missing")?;
    cache.complete(job, Ok(body(&s, 2)?), 42)?;
    let new = cache.read(id, 42).ok_or("cold")?;
    assert_eq!(new.revision(), 2);
    assert_eq!(old.revision(), 1);
    assert!(!Arc::ptr_eq(&old, &new));
    assert!(cache.read(id, 142).is_none());
    assert_eq!(
        cache.telemetry(id, 142).ok_or("missing metrics")?.age_ms,
        Some(100)
    );
    assert_eq!(cache.metrics().stale_reads, 1);
    Ok(())
}
#[test]
fn scope_generation_revision_and_accounting_fences() -> Result<(), Box<dyn std::error::Error>> {
    let mut cache = Cache::new(limits())?;
    let s = selected("g1")?;
    let id = cache.select(s.clone(), 0)?;
    let job = cache.begin(0).ok_or("job")?;
    cache.complete(job, Ok(body(&s, 2)?), 1)?;
    for (n, (path, replacement)) in [
        ("/profile/scope/tenant", json!("tenant-b")),
        ("/profile/scope/operation", json!("other")),
        ("/profile/scope/vendor", json!("other")),
        ("/profile/scope/side", json!("request")),
        ("/profile/scope/field", json!("/other")),
        ("/profile/scope/volume_cohort", json!("other")),
        (
            "/profile/scope/tenant_segment",
            json!("[\"ti48\",\"other-connection\",\"g1\",\"retail\",\"hour\"]"),
        ),
        (
            "/profile/scope/tenant_segment",
            json!("[\"ti48\",\"conn\",\"g1\",\"other-segment\",\"hour\"]"),
        ),
        ("/provenance", json!("forged")),
        ("/bucket", json!("2026-01-01T01:00:00.000Z")),
        ("/profile/end", json!("2026-01-01T02:00:00.000Z")),
        ("/generation", json!("g2")),
        ("/version", json!(2)),
        ("/revision", json!(1)),
        ("/profile/excluded_numbers", json!(999)),
        ("/profile/types/0", json!(9007199254740992_u64)),
        ("/profile/window_id", json!("equal-revision-conflict")),
    ]
    .into_iter()
    .enumerate()
    {
        let now = 41 + n as u64 * 60;
        cache.notify(id, now)?;
        let job = cache.begin(now).ok_or("job")?;
        // A newer revision proves validation itself rejects forged evidence;
        // equal-revision conflict must not accidentally mask a missing check.
        let revision = if path == "/profile/window_id" { 2 } else { 3 };
        let mut v: Value = serde_json::from_slice(&body(&s, revision)?)?;
        *v.pointer_mut(path).ok_or("path")? = replacement;
        assert!(
            cache
                .complete(job, Ok(serde_json::to_vec(&v)?), now + 1)
                .is_err(),
            "{path}"
        );
    }
    // Refresh faults never renew TTL; revision fences survive expiry.
    assert!(cache.read(id, 2000).is_none());
    let job = cache.begin(2000).ok_or("job")?;
    assert_eq!(
        cache.complete(job, Ok(body(&s, 1)?), 2001),
        Err(Error::Regression)
    );
    Ok(())
}
#[test]
fn delayed_completions_cannot_install_after_reselection_or_timeout()
-> Result<(), Box<dyn std::error::Error>> {
    let mut cache = Cache::new(limits())?;
    let s = selected("g1")?;
    let id = cache.select(s.clone(), 0)?;
    let delayed = cache.begin(0).ok_or("job")?;
    let new = cache.replace(id, selected("g2")?, 1)?;
    assert_eq!(
        cache.complete(delayed, Ok(body(&s, 1)?), 2),
        Err(Error::Obsolete)
    );
    assert!(cache.read(new, 2).is_none());
    let slow = cache.begin(2).ok_or("job")?;
    assert_eq!(
        cache.complete(slow, Ok(body(&selected("g2")?, 1)?), 12),
        Err(Error::Timeout)
    );
    assert!(cache.begin(16).is_none());
    let retry = cache.begin(17).ok_or("retry")?;
    assert_eq!(
        cache.complete(retry, Err(Error::Transport), 18),
        Err(Error::Transport)
    );
    cache.notify(new, 19)?;
    assert!(cache.begin(67).is_none());
    assert!(cache.begin(68).is_some());
    assert_eq!(cache.metrics().failures, 2);
    Ok(())
}
#[test]
fn bounded_reject_new_and_identical_revalidation() -> Result<(), Box<dyn std::error::Error>> {
    let mut cache = Cache::new(Limits {
        capacity: 1,
        ..limits()
    })?;
    let s = selected("g1")?;
    let id = cache.select(s.clone(), 0)?;
    assert_eq!(cache.select(selected("g2")?, 0), Err(Error::Capacity));
    let job = cache.begin(0).ok_or("job")?;
    cache.complete(job, Ok(body(&s, 1)?), 1)?;
    let job = cache.begin(41).ok_or("job")?;
    let pretty = serde_json::to_vec_pretty(&serde_json::from_slice::<Value>(&body(&s, 1)?)?)?;
    cache.complete(job, Ok(pretty), 42)?;
    assert!(cache.read(id, 101).is_some());
    let mut tight = Cache::new(Limits {
        bytes: 1,
        ..limits()
    })?;
    assert_eq!(tight.select(s, 0), Err(Error::Capacity));
    Ok(())
}

#[test]
fn reselecting_identical_slot_preserves_revision_and_notifications_coalesce()
-> Result<(), Box<dyn std::error::Error>> {
    let mut cache = Cache::new(limits())?;
    let s = selected("g1")?;
    let id = cache.select(s.clone(), 0)?;
    let job = cache.begin(0).ok_or("job")?;
    cache.complete(job, Ok(body(&s, 2)?), 1)?;
    assert_eq!(cache.replace(id, s.clone(), 2)?, id);
    cache.notify(id, 3)?;
    cache.notify(id, 3)?;
    let job = cache.begin(3).ok_or("job")?;
    assert!(cache.begin(3).is_none());
    assert_eq!(
        cache.complete(job, Ok(body(&s, 1)?), 4),
        Err(Error::Regression)
    );
    assert_eq!(cache.read(id, 4).ok_or("snapshot")?.revision(), 2);
    Ok(())
}

#[test]
fn abandoned_reservations_timeout_and_late_reply_cannot_complete_new_job()
-> Result<(), Box<dyn std::error::Error>> {
    let mut cache = Cache::new(limits())?;
    let s = selected("g1")?;
    let id = cache.select(s.clone(), 0)?;
    let abandoned = cache.begin(0).ok_or("job")?;
    assert!(cache.begin(10).is_none());
    let retry = cache.begin(15).ok_or("retry")?;
    assert_eq!(
        cache.complete(abandoned, Ok(body(&s, 99)?), 16),
        Err(Error::Obsolete)
    );
    cache.complete(retry, Ok(body(&s, 1)?), 17)?;
    assert_eq!(cache.read(id, 17).ok_or("snapshot")?.revision(), 1);
    assert_eq!(cache.metrics().failures, 1);
    Ok(())
}
