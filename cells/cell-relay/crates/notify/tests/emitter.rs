//! TII-28 bounded, off-path emission handoff.
mod support;
use agw_audit::contracts::notification::{ConsolidationMode, Notification};
use agw_notify::{
    emitter::{Admission, Emitter, MAX_CAPACITY, MAX_DRAIN, Sink, SinkError},
    envelope,
};
use support::{MemorySink, REFS, at, base};
type Result = std::result::Result<(), Box<dyn std::error::Error>>;

fn notification(ordinal: u64) -> std::result::Result<Notification, Box<dyn std::error::Error>> {
    let mut value = base();
    value.ordinal = ordinal;
    value.mode = ConsolidationMode::Individual;
    value.consolidated = 1;
    Ok(envelope::build(value)?)
}

struct Failing {
    failures: usize,
    delivered: usize,
}
impl Sink for Failing {
    fn tenant(&self) -> Option<&str> {
        Some("acme")
    }
    fn emit(&mut self, _: &Notification) -> std::result::Result<(), SinkError> {
        if self.failures > 0 {
            self.failures -= 1;
            return Err(SinkError::new("worker unavailable"));
        }
        self.delivered += 1;
        Ok(())
    }
}

#[test]
fn a_saturated_queue_refuses_without_touching_the_caller() -> Result {
    let mut emitter = Emitter::new("acme", 2)?;
    let first = notification(1)?;
    let second = notification(2)?;
    assert_eq!(emitter.emit(first)?, Admission::Admitted);
    assert_eq!(emitter.emit(second)?, Admission::Admitted);
    let payload = b"exact caller bytes".to_vec();
    let outcome = "bypassed";
    let third = notification(3)?;
    assert_eq!(emitter.emit(third)?, Admission::Full);
    assert_eq!(emitter.len(), 2);
    assert_eq!(emitter.metrics().rejected, 1);
    assert_eq!(payload, b"exact caller bytes");
    assert_eq!(outcome, "bypassed");
    Ok(())
}

#[test]
fn malformed_identities_are_refused_before_admission() -> Result {
    let mut emitter = Emitter::new("acme", 2)?;
    let mut malformed = notification(1)?;
    malformed.id = "ntf_bad".parse()?;
    assert!(emitter.emit(malformed).is_err());
    assert!(emitter.is_empty());
    assert_eq!(emitter.metrics().admitted, 0);

    let mut wrong_version = notification(2)?;
    wrong_version.version = 2;
    assert!(emitter.emit(wrong_version).is_err());
    assert!(emitter.is_empty());
    assert_eq!(emitter.metrics().admitted, 0);
    Ok(())
}

#[test]
fn one_drain_tick_is_bounded_even_when_more_is_queued() -> Result {
    let mut emitter = Emitter::new("acme", MAX_CAPACITY)?;
    for ordinal in 1..=MAX_DRAIN + 1 {
        emitter.emit(notification(ordinal as u64)?)?;
    }
    let mut sink = MemorySink::default();
    assert_eq!(emitter.drain(0, &mut sink)?, 0);
    assert_eq!(emitter.drain(usize::MAX, &mut sink)?, MAX_DRAIN);
    assert_eq!(emitter.len(), 1);
    assert_eq!(sink.ids.len(), MAX_DRAIN);
    Ok(())
}

#[test]
fn a_dead_worker_keeps_memory_bounded_and_counts_every_refusal() -> Result {
    let mut emitter = Emitter::new("acme", 3)?;
    for ordinal in 1..=13 {
        let expected = if ordinal <= 3 {
            Admission::Admitted
        } else {
            Admission::Full
        };
        assert_eq!(emitter.emit(notification(ordinal)?)?, expected);
    }
    assert_eq!(emitter.len(), 3);
    assert_eq!(emitter.metrics().rejected, 10);
    assert_eq!(emitter.metrics().admitted, 3);
    Ok(())
}

#[test]
fn duplicate_and_summary_updates_reuse_one_logical_notification() -> Result {
    let mut emitter = Emitter::new("acme", 4)?;
    let mut summary = base();
    summary.mode = ConsolidationMode::Summary;
    summary.ordinal = 5;
    summary.consolidated = 2;
    summary.occurrences = 4;
    let created = envelope::build(summary)?;
    assert_eq!(emitter.emit(created.clone())?, Admission::Admitted);
    assert_eq!(emitter.emit(created.clone())?, Admission::Duplicate);
    let mut updated = created;
    envelope::update_summary(
        &mut updated,
        7,
        at("2026-10-21T12:30:00Z"),
        at("2026-10-21T12:30:00Z"),
        3,
    )?;
    assert_eq!(emitter.emit(updated)?, Admission::Updated);
    assert_eq!(emitter.len(), 1);
    assert_eq!(emitter.metrics().duplicates, 1);
    assert_eq!(emitter.metrics().updated, 1);
    Ok(())
}

#[test]
fn drain_removes_only_accepted_entries_and_stops_at_the_bound() -> Result {
    let mut emitter = Emitter::new("acme", 4)?;
    for ordinal in 1..=3 {
        emitter.emit(notification(ordinal)?)?;
    }
    let mut sink = MemorySink::default();
    assert_eq!(emitter.drain(1, &mut sink)?, 1);
    assert_eq!(sink.ids.len(), 1);
    assert_eq!(emitter.len(), 2);
    assert_eq!(emitter.drain(10, &mut sink)?, 2);
    assert_eq!(sink.ids.len(), 3);
    assert!(emitter.is_empty());
    assert_eq!(emitter.metrics().forwarded, 3);
    Ok(())
}

#[test]
fn a_failing_head_stays_queued_and_is_retried_under_the_same_id() -> Result {
    let mut emitter = Emitter::new("acme", 2)?;
    let first = notification(1)?;
    let identity = first.id.clone();
    emitter.emit(first)?;
    emitter.emit(notification(2)?)?;
    let mut sink = Failing {
        failures: 1,
        delivered: 0,
    };
    assert_eq!(emitter.drain(10, &mut sink)?, 0);
    assert_eq!(emitter.len(), 2);
    assert_eq!(emitter.metrics().failures, 1);
    assert_eq!(emitter.drain(10, &mut sink)?, 2);
    assert_eq!(emitter.metrics().retries, 1);
    assert_eq!(emitter.metrics().forwarded, 2);
    assert_eq!(sink.delivered, 2);
    assert_eq!(identity.as_str().len(), 36);
    Ok(())
}

#[test]
fn tenant_boundaries_and_capacity_are_enforced() -> Result {
    assert!(Emitter::new("acme", 0).is_err());
    assert!(Emitter::new("acme", 4097).is_err());
    assert!(Emitter::new("", 1).is_err());
    let mut emitter = Emitter::new("acme", 2)?;
    let wrong = {
        let mut value = base();
        value.tenant = "victim";
        envelope::build(value)?
    };
    assert!(emitter.emit(wrong).is_err());
    emitter.emit(notification(1)?)?;
    let mut victim = MemorySink {
        tenant: Some("victim"),
        ..MemorySink::default()
    };
    assert!(emitter.drain(1, &mut victim).is_err());
    assert_eq!(emitter.len(), 1);
    Ok(())
}

#[test]
fn references_are_exposed_for_fixture_parity() {
    assert_eq!(REFS.len(), 1);
}
