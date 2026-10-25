use agw_guard::feedback::recovery::{Receipt, RetryOutcome, Tracker, TrackingError};
type TestResult = Result<(), TrackingError>;

#[test]
fn empty_window_has_no_rate() -> TestResult {
    let tracker = Tracker::new(2)?;
    assert_eq!(tracker.snapshot().recovery_rate(), None);
    assert_eq!(Tracker::new(0).err(), Some(TrackingError::InvalidCapacity));
    Ok(())
}

#[test]
fn retries_count_distinct_blocked_calls_and_only_verified_success_recovers() -> TestResult {
    let mut tracker = Tracker::new(3)?;
    assert_eq!(tracker.blocked("tool", "aud_a")?, Receipt::Recorded);
    assert_eq!(tracker.blocked("tool", "aud_a")?, Receipt::Duplicate);
    tracker.blocked("tool", "aud_b")?;
    tracker.blocked("tool", "aud_c")?;
    tracker.retry("tool", "aud_a", RetryOutcome::Failed)?;
    tracker.retry("tool", "aud_a", RetryOutcome::Bypassed)?;
    assert_eq!(tracker.snapshot().recovery_rate(), Some(0.0));
    assert_eq!(
        tracker.retry("tool", "aud_a", RetryOutcome::VerifiedTaskSuccess)?,
        Receipt::Recorded
    );
    assert_eq!(
        tracker.retry("tool", "aud_a", RetryOutcome::VerifiedTaskSuccess)?,
        Receipt::Duplicate
    );
    tracker.retry("tool", "aud_b", RetryOutcome::Failed)?;
    tracker.retry("tool", "aud_b", RetryOutcome::Bypassed)?;
    let metrics = tracker.snapshot();
    assert_eq!(metrics.blocked, 3);
    assert_eq!(metrics.retried, 2);
    assert_eq!(metrics.recovered, 1);
    assert_eq!(metrics.recovery_rate(), Some(0.5));
    // A late failed callback must never erase a verified success.
    tracker.retry("tool", "aud_a", RetryOutcome::Failed)?;
    assert_eq!(tracker.snapshot(), metrics);
    Ok(())
}

#[test]
fn unknown_scope_and_full_windows_are_explicit_and_atomic() -> TestResult {
    let mut tracker = Tracker::new(1)?;
    tracker.blocked("tool", "aud_a")?;
    assert_eq!(tracker.blocked("tool", "aud_b"), Err(TrackingError::Full));
    assert_eq!(tracker.blocked("tool", "aud_a"), Ok(Receipt::Duplicate));
    assert_eq!(
        tracker.retry("other_tool", "aud_a", RetryOutcome::VerifiedTaskSuccess),
        Err(TrackingError::UnknownBlock)
    );
    assert_eq!(
        tracker.retry("tool", "aud_b", RetryOutcome::Failed),
        Err(TrackingError::UnknownBlock)
    );
    assert_eq!(
        tracker.blocked("", "aud_a"),
        Err(TrackingError::InvalidIdentity)
    );
    assert_eq!(
        tracker.blocked("tool", &"a".repeat(513)),
        Err(TrackingError::InvalidIdentity)
    );
    assert_eq!(tracker.snapshot().blocked, 1);
    assert_eq!(tracker.snapshot().retried, 0);
    Ok(())
}
