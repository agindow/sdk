use agw_diff::{Change, DriftClass, Kind, Reason, Report, Side};
use agw_stats::investigation::{Exporter, Outcome};
use agw_stats::router::{CallBudget, DeliveryContext, Outbox, route};

fn queued(call: &str) -> Result<Outbox, Box<dyn std::error::Error>> {
    let report = Report {
        changes: vec![Change {
            side: Side::Response,
            path: "amount".into(),
            to_path: None,
            kind: Kind::Removed,
            class: DriftClass::Unknown,
            reason: Reason::InsufficientEvidence,
        }],
        skipped: None,
    };
    let context = DeliveryContext::new("tenant-a", call, "price-test", 1792584000000)?;
    let mut budget = CallBudget::new("audit-test", u64::MAX)?.with_delivery(context)?;
    let mut outbox = Outbox::new(1)?;
    outbox.submit(&route(&report, &[])?, &mut budget, u64::MAX)?;
    Ok(outbox)
}
#[test]
fn background_delivery_preserves_scope_and_replays_identical_bytes()
-> Result<(), Box<dyn std::error::Error>> {
    let mut source = queued("call-test")?;
    let mut exporter = Exporter::new(1)?;
    assert!(exporter.collect_one(&mut source)?);
    assert!(source.is_empty());
    let now = 1792584000000;
    let mut bodies = Vec::new();
    exporter.pump(now, |bytes| {
        bodies.push(bytes.to_vec());
        Outcome::Uncertain
    });
    exporter.pump(now + 999, |_| panic!("backoff must skip send"));
    exporter.pump(now + 1000, |bytes| {
        bodies.push(bytes.to_vec());
        Outcome::Accepted
    });
    assert_eq!(bodies.len(), 2);
    assert_eq!(bodies[0], bodies[1]);
    let value: serde_json::Value = serde_json::from_slice(&bodies[0])?;
    assert_eq!(
        value["id"],
        "inv_8294b408765f9737d487381f77a5e0c8e258820295448bfe42d32d581fcfb936"
    );
    assert_eq!(value["ceiling"], u64::MAX.to_string());
    assert_eq!(value["currency"], "USD");
    assert_eq!(value["tenant"], "tenant-a");
    assert_eq!(value["call_id"], "call-test");
    assert_eq!(value["price_context"], "price-test");
    assert!(value.get("payload").is_none());
    agw_audit::investigation_wire::validate(&bodies[0])?;
    let counts = exporter.counters();
    assert_eq!(counts.delivered, 1);
    assert_eq!(counts.retried, 1);
    assert_eq!(
        exporter.terminal().back().map(|record| record.status),
        Some("durable-accepted")
    );
    Ok(())
}
#[test]
fn bounded_exporter_retains_source_and_reports_expiry_dead_letters_and_shutdown_uncertainty()
-> Result<(), Box<dyn std::error::Error>> {
    let now = 1792584000000;
    let mut exporter = Exporter::new(1)?;
    exporter.collect_one(&mut queued("first")?)?;
    let mut second = queued("second")?;
    assert!(!exporter.collect_one(&mut second)?);
    assert_eq!(second.len(), 1);
    exporter.pump(now + 600000, |_| panic!("expired work must not send"));
    assert_eq!(exporter.counters().expired, 1);
    exporter.collect_one(&mut second)?;
    exporter.pump(now, |_| Outcome::Rejected);
    assert_eq!(exporter.counters().dead_letter, 1);
    exporter.collect_one(&mut queued("third")?)?;
    exporter.pump(now, |_| Outcome::Uncertain);
    let losses = exporter.shutdown();
    assert_eq!(losses.len(), 1);
    assert_eq!(losses[0].status, "uncertain-local-loss");
    assert_eq!(losses[0].audit_ref, "audit-test");
    assert_eq!(exporter.counters().dropped, 1);
    assert!(exporter.is_empty());
    Ok(())
}
#[test]
fn local_spend_cannot_be_reauthorized_by_hosted_delivery() -> Result<(), Box<dyn std::error::Error>>
{
    let mut source = queued("spent")?;
    let mut job = source.pop().ok_or("missing job")?;
    let _receipt = job.authorize(1)?.settle(1);
    let mut exporter = Exporter::new(1)?;
    assert!(exporter.stage(job).is_err());
    assert_eq!(exporter.counters().dropped, 1);
    assert!(DeliveryContext::new("bad/tenant", "call", "price", 0).is_err());
    assert!(Exporter::new(1025).is_err());
    Ok(())
}
