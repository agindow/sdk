#[path = "support/statistical.rs"]
mod support;
use agw_diff::Side;
use agw_guard::statistical::{Engine, Operation, Plan};
use agw_stats::{
    refresh::{Cache, Error, Limits, Selected},
    windows::Availability,
};
use support::{body, context, field, identity};
#[test]
fn remote_aggregates_cannot_make_guard_ready_or_change_original_outcomes()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = include_bytes!("../../../../../contracts/examples/profile_snapshot.valid.json");
    let v: serde_json::Value = serde_json::from_slice(fixture)?;
    let selected: Selected =
        serde_json::from_value(serde_json::json!({"scope":v["profile"]["scope"],
        "generation":v["generation"],"grain":v["grain"],"bucket":v["bucket"],
        "start":v["profile"]["start"],"end":v["profile"]["end"]}))?;
    let mut cache = Cache::new(Limits {
        capacity: 1,
        bytes: 262144,
        ttl_ms: 100,
        refresh_ms: 40,
        timeout_ms: 10,
        backoff_ms: 5,
        recovery_ms: 50,
        attempts: 2,
    })?;
    let id = cache.select(selected, 0)?;
    for now in [0, 1, 100, 150] {
        if now == 1 {
            let job = cache.begin(0).ok_or("job")?;
            cache.complete(job, Ok(fixture.to_vec()), 1)?;
        }
        if now == 100 {
            let job = cache.begin(100).ok_or("job")?;
            assert_eq!(
                cache.complete(job, Err(Error::Transport), 100),
                Err(Error::Transport)
            );
        }
        for mode in ["observe", "repair", "strict"] {
            let exact = cache.read(id, now).and_then(|s| s.exact_detector_inputs());
            assert!(exact.is_none());
            let mut f = field(Side::Request, "1", "100")?;
            f.snapshot = exact.map_or(
                Availability::Unavailable(agw_stats::windows::Error::ProviderUnavailable),
                Availability::Ready,
            );
            let engine = Engine::statistical(Plan::new(identity(), vec![f])?);
            let original = Box::new("original vendor error");
            let ptr = &*original as *const &str;
            let output = engine
                .begin_call(context(mode)?, Operation::Write, "audit", 100_000_000, now)
                .execute(body(b"{}"), None, || (Err::<(), _>(original), body(b"{}")));
            assert_eq!(output.bypasses, 1);
            let error = output
                .outcome
                .ok_or("missing outcome")?
                .err()
                .ok_or("missing error")?;
            assert_eq!(&*error as *const &str, ptr);
        }
    }
    // Guard calls only read locally; only the two explicit background jobs ran.
    assert_eq!(cache.metrics().attempts, 2);
    Ok(())
}
