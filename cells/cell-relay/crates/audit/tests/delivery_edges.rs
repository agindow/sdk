mod support;
use agw_audit::{
    delivery::{HttpsSender, Limits, Outbox},
    publish::{Publisher, PublisherConfig},
    sink::{Sink, SinkError},
};
use std::{collections::BTreeMap, error::Error, fs};
use support::{MemorySink, fixture, spool_path as path};
type Result = std::result::Result<(), Box<dyn Error>>;
#[test]
fn concurrent_spool_paths_have_distinct_process_local_identities() -> Result {
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(16));
    let workers: Vec<_> = (0..16)
        .map(|_| {
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                (0..256).map(|_| path()).collect::<Vec<_>>()
            })
        })
        .collect();
    let mut paths = std::collections::BTreeSet::new();
    for worker in workers {
        for path in worker
            .join()
            .map_err(|_| "path allocator worker panicked")?
        {
            assert!(
                paths.insert(path),
                "parallel tests must never share a spool path"
            );
        }
    }
    assert_eq!(paths.len(), 4096);
    Ok(())
}
struct Ambiguous {
    records: BTreeMap<String, Vec<u8>>,
    received: usize,
}
impl Sink for Ambiguous {
    fn emit(
        &mut self,
        artifact: &agw_audit::contracts::audit_artifact::AuditArtifact,
    ) -> std::result::Result<(), SinkError> {
        let bytes = serde_json::to_vec(artifact).map_err(|_| SinkError::new("serialize"))?;
        let id = artifact.id.as_str().to_owned();
        if let Some(previous) = self.records.get(&id) {
            assert_eq!(&bytes, previous);
        }
        self.records.insert(id, bytes);
        self.received += 1;
        if self.received == 1 {
            Err(SinkError::new("acknowledgement lost"))
        } else {
            Ok(())
        }
    }
}
#[test]
fn ambiguous_delivery_replays_identical_bytes_after_restart() -> Result {
    let path = path();
    let limits = Limits {
        backoff_ms: 1,
        ..Limits::default()
    };
    let mut spool = Outbox::open(&path, "acme", limits)?;
    let a = fixture()?;
    spool.enqueue("acme", &a)?;
    spool.enqueue("acme", &a)?;
    assert_eq!(spool.len(), 1);
    let mut conflicting = a.clone();
    conflicting.tool = "changed".try_into()?;
    assert!(spool.enqueue("acme", &conflicting).is_err());
    let mut sink = Ambiguous {
        records: BTreeMap::new(),
        received: 0,
    };
    assert_eq!(spool.drain(0, 10, &mut sink)?, 0);
    drop(spool);
    let mut spool = Outbox::open(&path, "acme", limits)?;
    assert_eq!(spool.drain(1, 10, &mut sink)?, 1);
    assert_eq!(sink.records.len(), 1);
    assert_eq!(sink.received, 2);
    drop(spool);
    let spool = Outbox::open(&path, "acme", limits)?;
    assert!(spool.is_empty());
    assert_eq!(spool.metrics().deliveries, 1);
    assert_eq!(spool.metrics().retries, 1);
    assert_eq!(spool.metrics().drains, 2);
    assert!(spool.metrics().duration_ns > 0);
    drop(spool);
    fs::remove_file(path)?;
    Ok(())
}
#[test]
fn destination_and_publisher_tenant_fences_cover_missing_metadata() -> Result {
    let path = path();
    let mut spool = Outbox::open(&path, "acme", Limits::default())?;
    let mut a = fixture()?;
    a.metadata.remove("tenant");
    spool.enqueue("acme", &a)?;
    let mut wrong = HttpsSender::new(
        "https://example.test/v1/audit",
        "victim",
        "test-only-credential-with-32-bytes",
        1000,
    )?;
    assert!(spool.drain(0, 1, &mut wrong).is_err());
    assert_eq!(spool.len(), 1);
    let mut publisher = Publisher::new("victim", PublisherConfig::default())?;
    publisher.publish(a.clone(), a.ts)?;
    assert!(publisher.stage(&mut spool, 1).is_err());
    assert_eq!(publisher.pending(), 1);
    drop(spool);
    fs::remove_file(path)?;
    Ok(())
}
#[test]
fn bytes_artifact_and_reopen_bounds_fail_closed_without_eviction() -> Result {
    let path = path();
    let limits = Limits {
        bytes: 1024,
        ..Limits::default()
    };
    let mut spool = Outbox::open(&path, "acme", limits)?;
    assert!(spool.enqueue("acme", &fixture()?).is_err());
    assert!(spool.is_empty());
    assert_eq!(spool.metrics().rejected, 1);
    drop(spool);
    fs::remove_file(&path)?;
    let mut spool = Outbox::open(&path, "acme", Limits::default())?;
    let mut a = fixture()?;
    a.metadata
        .insert("large".into(), serde_json::json!("x".repeat(1_048_576)));
    assert!(spool.enqueue("acme", &a).is_err());
    a.metadata.remove("large");
    spool.enqueue("acme", &a)?;
    assert_eq!(spool.drain(0, 0, &mut MemorySink(vec![]))?, 0);
    assert_eq!(spool.len(), 1);
    drop(spool);
    assert!(Outbox::open(&path, "acme", limits).is_err());
    fs::write(&path, b"not a spool")?;
    assert!(Outbox::open(&path, "acme", Limits::default()).is_err());
    fs::remove_file(path)?;
    Ok(())
}
#[test]
fn filesystem_fault_retains_publisher_front_and_requires_reopen() -> Result {
    let path = path();
    let mut spool = Outbox::open(&path, "acme", Limits::default())?;
    let mut publisher = Publisher::new("acme", PublisherConfig::default())?;
    let a = fixture()?;
    publisher.publish(a.clone(), a.ts)?;
    fs::create_dir(&path)?;
    assert!(publisher.stage(&mut spool, 1).is_err());
    assert_eq!(publisher.pending(), 1);
    fs::remove_dir(&path)?;
    assert!(publisher.stage(&mut spool, 1).is_err());
    drop(spool);
    let mut spool = Outbox::open(&path, "acme", Limits::default())?;
    assert_eq!(publisher.stage(&mut spool, 1)?, 1);
    drop(spool);
    fs::remove_file(path)?;
    Ok(())
}
#[test]
fn shared_fixture_is_exactly_the_real_publication() -> Result {
    let a = fixture()?;
    let mut publisher = Publisher::new("acme", PublisherConfig::default())?;
    publisher.publish(a.clone(), a.ts)?;
    let mut sink = MemorySink(vec![]);
    publisher.drain(&mut sink, 1);
    let expected: serde_json::Value =
        serde_json::from_str(include_str!("support/relay-audit.json"))?;
    assert_eq!(serde_json::to_value(&sink.0[0])?, expected);
    Ok(())
}
