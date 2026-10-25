#[path = "support/statistical.rs"]
mod support;
use agw_diff::{Evidence, Node, Snapshot};
use agw_guard::statistical::{Decision, Engine, Operation, Plan};
use agw_stats::{
    investigation::{Exporter, Outcome},
    router::{DeliveryContext, Outbox},
};
use support::{body, context, identity};

#[test]
fn stalled_export_never_delays_guard_and_retains_original_outcomes()
-> Result<(), Box<dyn std::error::Error>> {
    let mut plan = Plan::new(identity(), vec![])?;
    let shape = Snapshot {
        node: Node::object([]),
        evidence: Evidence::Learned,
    };
    plan.request_shape = Some(shape.clone());
    plan.response_shape = Some(shape);
    plan.investigation_ceiling = 5;
    let engine = Engine::statistical(plan);
    let mut outbox = Outbox::new(1)?;
    let now = 1792584000000;
    let delivery = || DeliveryContext::new("tenant", "call-test", "price-test", now);
    let first = engine
        .begin_call(
            context("repair")?,
            Operation::Read,
            "audit-test",
            100_000_000,
            10,
        )
        .with_delivery(delivery()?)?
        .execute(body(b"{\"extra\":1}"), Some(&mut outbox), || {
            ("original", body(b"{\"extra\":2}"))
        });
    assert_eq!(first.outcome, Some("original"));
    let mut exporter = Exporter::new(1)?;
    exporter.collect_one(&mut outbox)?;
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let handle = std::thread::spawn(move || {
        exporter.pump(now, |_| {
            let _ = started_tx.send(());
            let _ = release_rx.recv();
            Outcome::Uncertain
        });
        exporter
    });
    started_rx.recv_timeout(std::time::Duration::from_secs(2))?;
    let bytes = b"{ \"extra\": 3 }";
    let mut upstream = 0;
    for _ in 0..3 {
        let result = engine
            .begin_call(
                context("repair")?,
                Operation::Read,
                "next-audit",
                100_000_000,
                10,
            )
            .with_delivery(delivery()?)?
            .execute(body(bytes), Some(&mut outbox), || {
                upstream += 1;
                (bytes.as_slice(), body(bytes))
            });
        assert_eq!(result.request.decision, Decision::Degraded);
        assert!(std::ptr::eq(
            result.outcome.ok_or("missing original")?,
            bytes.as_slice()
        ));
    }
    assert_eq!(upstream, 3);
    assert_eq!(outbox.len(), 1);
    release_tx.send(())?;
    assert_eq!(
        handle
            .join()
            .map_err(|_| "background worker panic")?
            .counters()
            .retried,
        1
    );
    Ok(())
}
