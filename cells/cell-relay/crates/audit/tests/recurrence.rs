//! TI-29 recurrence: first_seen/occurrences per drift signature, locally bounded.
mod support;

use agw_audit::contracts::audit_artifact::{Change, DriftSide};
use agw_audit::{Error, recurrence};
use chrono::{Duration, TimeZone, Utc};
use std::error::Error as StdError;

type Result = std::result::Result<(), Box<dyn StdError>>;

/// 2026-10-17T14:00:11Z, the fixture's own timestamp.
fn first() -> chrono::DateTime<Utc> {
    const SECONDS: i64 = 1_761_110_411;
    Utc.timestamp_opt(SECONDS, 0)
        .single()
        .unwrap_or_else(Utc::now)
}

#[test]
fn the_first_publication_stamps_first_seen_and_one_occurrence() -> Result {
    let mut drift = support::fixture()?.drift.ok_or("missing drift")?;
    let mut recurrence = recurrence::Recurrence::new(8)?;
    recurrence.record(&mut drift, first())?;
    assert_eq!(drift.first_seen, Some(first()));
    assert_eq!(drift.occurrences.map(u64::from), Some(1));
    assert_eq!(recurrence.len(), 1);
    Ok(())
}

#[test]
fn repeated_signatures_keep_first_seen_and_increment_occurrences() -> Result {
    let mut drift = support::fixture()?.drift.ok_or("missing drift")?;
    let mut recurrence = recurrence::Recurrence::new(8)?;
    recurrence.record(&mut drift, first())?;
    let later = first() + Duration::hours(3);
    recurrence.record(&mut drift, later)?;
    recurrence.record(&mut drift, later + Duration::hours(1))?;
    assert_eq!(drift.first_seen, Some(first()));
    assert_eq!(drift.occurrences.map(u64::from), Some(3));
    assert_eq!(recurrence.len(), 1);
    Ok(())
}

#[test]
fn change_order_does_not_split_one_signature() -> Result {
    let mut drift = support::fixture()?.drift.ok_or("missing drift")?;
    let mut changes = drift.changes.clone();
    changes.reverse();
    let mut recurrence = recurrence::Recurrence::new(8)?;
    recurrence.record(&mut drift, first())?;
    let mut reordered = drift.clone();
    reordered.changes = changes;
    recurrence.record(&mut reordered, first())?;
    assert_eq!(reordered.occurrences.map(u64::from), Some(2));
    assert_eq!(recurrence.len(), 1);
    Ok(())
}

#[test]
fn class_side_and_change_identity_are_distinct_signatures() -> Result {
    let fixture = support::fixture()?;
    let base = fixture.drift.ok_or("missing drift")?;
    let mut recurrence = recurrence::Recurrence::new(8)?;
    let mut a = base.clone();
    recurrence.record(&mut a, first())?;

    let mut other_side = base.clone();
    other_side.side = DriftSide::Both;
    recurrence.record(&mut other_side, first())?;
    assert_eq!(other_side.occurrences.map(u64::from), Some(1));

    let mut other_change = base.clone();
    other_change.changes = vec![Change {
        path: "amount".try_into()?,
        kind: "type_changed".try_into()?,
        from: None,
        to: None,
    }];
    recurrence.record(&mut other_change, first())?;
    assert_eq!(other_change.occurrences.map(u64::from), Some(1));
    assert_eq!(recurrence.len(), 3);
    Ok(())
}

#[test]
fn full_capacity_refuses_without_mutating_the_drift() -> Result {
    let fixture = support::fixture()?;
    let base = fixture.drift.ok_or("missing drift")?;
    let mut recurrence = recurrence::Recurrence::new(1)?;
    let mut a = base.clone();
    recurrence.record(&mut a, first())?;

    let mut b = base.clone();
    // The fixture ships pre-stamped; this drift arrives unpublished.
    b.first_seen = None;
    b.occurrences = None;
    b.changes = vec![Change {
        path: "other".try_into()?,
        kind: "field_removed".try_into()?,
        from: None,
        to: None,
    }];
    assert!(matches!(
        recurrence.record(&mut b, first()),
        Err(Error::Capacity)
    ));
    assert_eq!(b.first_seen, None);
    assert_eq!(b.occurrences, None);
    assert_eq!(recurrence.len(), 1);
    Ok(())
}

#[test]
fn capacity_bounds_are_validated() {
    assert!(matches!(
        recurrence::Recurrence::new(0),
        Err(Error::Invalid(_))
    ));
    assert!(matches!(
        recurrence::Recurrence::new(recurrence::MAX_CAPACITY + 1),
        Err(Error::Invalid(_))
    ));
    assert!(recurrence::Recurrence::new(recurrence::MAX_CAPACITY).is_ok());
}
