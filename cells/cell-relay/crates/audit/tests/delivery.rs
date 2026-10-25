mod support;
use agw_audit::{
    delivery::{Limits, Outbox},
    publish::{Publisher, PublisherConfig},
    sink::{NdjsonSink, Sink, SinkError},
};
use std::{
    error::Error,
    fs,
    sync::{Arc, Mutex},
};
use support::{FailingSink, MemorySink, fixture, spool_path as path};
type Result = std::result::Result<(), Box<dyn Error>>;
#[test]
fn publisher_transfer_is_durable_ordered_and_tenant_bound() -> Result {
    let path = path();
    let mut publisher = Publisher::new("acme", PublisherConfig::default())?;
    let a = fixture()?;
    publisher.publish(a.clone(), a.ts)?;
    let mut b = a.clone();
    b.id = "second".try_into()?;
    publisher.publish(b, a.ts)?;
    let mut spool = Outbox::open(&path, "acme", Limits::default())?;
    assert_eq!(publisher.stage(&mut spool, 1)?, 1);
    assert_eq!(publisher.pending(), 1);
    drop(spool);
    let mut spool = Outbox::open(&path, "acme", Limits::default())?;
    assert_eq!(publisher.stage(&mut spool, 10)?, 1);
    let mut sink = MemorySink(vec![]);
    assert_eq!(spool.drain(0, 10, &mut sink)?, 2);
    assert_eq!(
        sink.0.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(),
        vec![a.id.as_str(), "second"]
    );
    drop(spool);
    fs::remove_file(path)?;
    Ok(())
}
#[test]
fn retry_budget_and_head_backoff_survive_restart() -> Result {
    let path = path();
    let limits = Limits {
        attempts: 2,
        backoff_ms: 10,
        ..Limits::default()
    };
    let mut spool = Outbox::open(&path, "acme", limits)?;
    spool.enqueue("acme", &fixture()?)?;
    let mut second = fixture()?;
    second.id = "second".try_into()?;
    spool.enqueue("acme", &second)?;
    assert_eq!(spool.drain(0, 100, &mut FailingSink)?, 0);
    drop(spool);
    let mut spool = Outbox::open(&path, "acme", limits)?;
    let mut sink = MemorySink(vec![]);
    assert_eq!(spool.drain(9, 100, &mut sink)?, 0);
    assert!(sink.0.is_empty());
    spool.drain(10, 100, &mut FailingSink)?;
    assert_eq!(spool.drain(29, 100, &mut sink)?, 0);
    assert_eq!(spool.drain(30, 1, &mut sink)?, 0);
    assert_eq!(spool.metrics().drops, 1);
    assert_eq!(spool.drain(30, 1, &mut sink)?, 1);
    assert_eq!(sink.0[0].id.as_str(), "second");
    assert_eq!(spool.metrics().retries, 1);
    drop(spool);
    fs::remove_file(path)?;
    Ok(())
}
#[test]
fn pressure_retains_publisher_front_and_counts_rejection() -> Result {
    let path = path();
    let mut spool = Outbox::open(
        &path,
        "acme",
        Limits {
            entries: 1,
            ..Limits::default()
        },
    )?;
    spool.enqueue("acme", &fixture()?)?;
    let mut a = fixture()?;
    a.id = "next".try_into()?;
    let mut publisher = Publisher::new(
        "acme",
        PublisherConfig {
            outbox_capacity: 1,
            ..PublisherConfig::default()
        },
    )?;
    publisher.publish(a.clone(), a.ts)?;
    assert!(publisher.stage(&mut spool, 1).is_err());
    assert_eq!(publisher.pending(), 1);
    a.id = "dropped".try_into()?;
    assert!(!publisher.publish(a.clone(), a.ts)?.queued);
    assert_eq!(publisher.dropped(), 1);
    let metrics = publisher.metrics().export_json(a.ts);
    let points = metrics["resourceMetrics"][0]["scopeMetrics"][0]["metrics"]
        .as_array()
        .ok_or("metrics")?;
    let loss = points
        .iter()
        .find(|m| m["name"] == "audit_publication_drops_total")
        .ok_or("drop metric")?;
    assert_eq!(loss["sum"]["dataPoints"][0]["asInt"], "1");
    assert_eq!(spool.metrics().rejected, 1);
    assert!(spool.enqueue("victim", &a).is_err());
    assert!(Outbox::open(&path, "acme", Limits::default()).is_err());
    drop(spool);
    assert!(Outbox::open(&path, "victim", Limits::default()).is_err());
    fs::remove_file(path)?;
    Ok(())
}
#[test]
fn concurrent_drains_serialize_and_ndjson_framing_survives() -> Result {
    let path = path();
    let mut spool = Outbox::open(&path, "acme", Limits::default())?;
    spool.enqueue("acme", &fixture()?)?;
    let spool = Arc::new(Mutex::new(spool));
    let output = Arc::new(Mutex::new(MemorySink(vec![])));
    std::thread::scope(|s| {
        for _ in 0..2 {
            let spool = Arc::clone(&spool);
            let output = Arc::clone(&output);
            s.spawn(move || -> std::io::Result<()> {
                spool
                    .lock()
                    .map_err(|_| std::io::Error::other("lock"))?
                    .drain(
                        0,
                        10,
                        &mut *output.lock().map_err(|_| std::io::Error::other("lock"))?,
                    )?;
                Ok(())
            });
        }
    });
    assert_eq!(output.lock().map_err(|_| "lock")?.0.len(), 1);
    let mut spool = spool.lock().map_err(|_| "lock")?;
    let mut a = fixture()?;
    a.id = "ndjson".try_into()?;
    spool.enqueue("acme", &a)?;
    let mut sink = NdjsonSink::new(Vec::new());
    assert_eq!(spool.drain(0, 1, &mut sink)?, 1);
    let bytes = sink.into_inner();
    assert_eq!(bytes.last(), Some(&b'\n'));
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&bytes)?["id"],
        "ndjson"
    );
    drop(spool);
    drop(output);
    // Release the Arc's owner before removing the data file.
    fs::remove_file(path)?;
    Ok(())
}
struct Conflict;
impl Sink for Conflict {
    fn emit(
        &mut self,
        _: &agw_audit::contracts::audit_artifact::AuditArtifact,
    ) -> std::result::Result<(), SinkError> {
        Err(SinkError::conflict())
    }
}
#[test]
fn terminal_conflict_is_observable_and_does_not_block_the_next_item() -> Result {
    let path = path();
    let mut spool = Outbox::open(&path, "acme", Limits::default())?;
    spool.enqueue("acme", &fixture()?)?;
    assert_eq!(spool.drain(0, 1, &mut Conflict)?, 0);
    assert_eq!(spool.metrics().conflicts, 1);
    assert_eq!(spool.metrics().drops, 1);
    let json = spool.metrics().export_json(chrono::Utc::now());
    assert!(json.to_string().contains("audit_delivery_conflicts_total"));
    assert!(json.to_string().contains("audit_drain_duration"));
    drop(spool);
    fs::remove_file(path)?;
    Ok(())
}
