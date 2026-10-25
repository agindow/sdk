use agw_diff::{Change, DriftClass, Kind, Reason, Report, Side, Skip};
use agw_stats::router::{Admission, CallBudget, Error, Outbox, route};

fn report(class: DriftClass) -> Report {
    Report {
        changes: vec![Change {
            side: Side::Request,
            path: "x".into(),
            to_path: None,
            kind: Kind::Removed,
            class,
            reason: Reason::InsufficientEvidence,
        }],
        skipped: None,
    }
}

#[test]
fn queue_is_bounded_fifo_and_failed_admission_never_charges() -> Result<(), Error> {
    let report = report(DriftClass::Unknown);
    let r = route(&report, &[])?;
    let mut queue = Outbox::new(1)?;
    let mut first = CallBudget::new("tenant/call-1", 10)?;
    let mut second = CallBudget::new("tenant/call-2", 10)?;
    assert_eq!(queue.submit(&r, &mut first, 10)?, Admission::Enqueued);
    assert_eq!(first.reserved(), 10);
    assert_eq!(queue.len(), 1);
    assert_eq!(queue.submit(&r, &mut second, 10)?, Admission::Full);
    assert_eq!(second.reserved(), 0);
    let job = queue.pop().ok_or(Error::InvalidInput)?;
    assert_eq!(job.audit_ref(), "tenant/call-1");
    assert_eq!(job.cap(), 10);
    assert_eq!(job.authorized(), 0);
    assert_eq!(
        queue.submit(&r, &mut first, 1)?,
        Admission::AlreadySubmitted
    );
    assert_eq!(queue.submit(&r, &mut second, 10)?, Admission::Enqueued);
    assert_eq!(
        queue.pop().ok_or(Error::InvalidInput)?.audit_ref(),
        "tenant/call-2"
    );
    assert!(queue.pop().is_none());
    Ok(())
}

#[test]
fn known_clean_skipped_and_disabled_cases_never_reserve() -> Result<(), Error> {
    let mut queue = Outbox::new(1)?;
    let mut call = CallBudget::new("t/c", 10)?;
    for class in [DriftClass::Shape, DriftClass::Semantic] {
        assert_eq!(
            queue.submit(&route(&report(class), &[])?, &mut call, 10)?,
            Admission::NotNeeded
        );
    }
    assert_eq!(
        queue.submit(&route(&Report::default(), &[])?, &mut call, 10)?,
        Admission::NotNeeded
    );
    let mut unknown = report(DriftClass::Unknown);
    unknown.skipped = Some(Skip::Incomplete);
    assert_eq!(
        queue.submit(&route(&unknown, &[])?, &mut call, 10)?,
        Admission::NotNeeded
    );
    unknown.skipped = None;
    assert_eq!(
        Outbox::new(0)?.submit(&route(&unknown, &[])?, &mut call, 10)?,
        Admission::Disabled
    );
    assert_eq!(call.reserved(), 0);
    assert!(queue.is_empty());
    assert!(Outbox::new(1025).is_err());
    Ok(())
}

#[test]
fn budgets_cover_both_phases_and_reject_zero_or_excess_cost() -> Result<(), Error> {
    let report = report(DriftClass::Unknown);
    let r = route(&report, &[])?;
    let mut queue = Outbox::new(2)?;
    let mut call = CallBudget::new("t/c", 9)?;
    assert_eq!(queue.submit(&r, &mut call, 10)?, Admission::BudgetExhausted);
    assert_eq!(call.reserved(), 0);
    assert_eq!(queue.submit(&r, &mut call, 0), Err(Error::InvalidInput));
    assert_eq!(queue.submit(&r, &mut call, 9)?, Admission::Enqueued);
    assert_eq!(queue.submit(&r, &mut call, 1)?, Admission::AlreadySubmitted);
    assert_eq!(queue.len(), 1);
    assert_eq!(call.reserved(), 9);
    assert_eq!(
        queue.submit(&r, &mut CallBudget::new("t/zero", 0)?, 1)?,
        Admission::BudgetExhausted
    );
    let mut maximum = CallBudget::new("t/max", u64::MAX)?;
    queue.submit(&r, &mut maximum, u64::MAX)?;
    assert_eq!(maximum.reserved(), u64::MAX);
    assert!(CallBudget::new("", 1).is_err());
    assert!(CallBudget::new(&"a".repeat(4097), 1).is_err());
    Ok(())
}

#[test]
fn background_grants_are_bounded_and_receipts_measure_overruns_honestly() -> Result<(), Error> {
    let report = report(DriftClass::Unknown);
    let mut queue = Outbox::new(1)?;
    queue.submit(&route(&report, &[])?, &mut CallBudget::new("t/c", 10)?, 10)?;
    let mut job = queue.pop().ok_or(Error::InvalidInput)?;
    assert_eq!(job.authorize(0).err(), Some(Error::InvalidInput));
    let first = job.authorize(6)?;
    assert_eq!(job.authorized(), 6);
    assert_eq!(job.authorize(5).err(), Some(Error::BudgetExhausted));
    assert_eq!(job.authorized(), 6);
    let second = job.authorize(4)?;
    let receipt = first.settle(3);
    assert_eq!(receipt.audit_ref, "t/c");
    assert_eq!(receipt.authorized, 6);
    assert_eq!(receipt.actual, 3);
    assert!(!receipt.overrun());
    assert_eq!(job.authorize(1).err(), Some(Error::BudgetExhausted));
    let receipt = second.settle(u64::MAX);
    assert_eq!(receipt.actual, u64::MAX);
    assert!(receipt.overrun());
    assert_eq!(job.authorized(), 10);
    Ok(())
}

#[test]
fn request_and_response_share_one_budget_and_queue_preserves_order() -> Result<(), Error> {
    let shape = report(DriftClass::Shape);
    let mut response = report(DriftClass::Unknown);
    response.changes[0].side = Side::Response;
    let unknown = route(&response, &[])?;
    let mut queue = Outbox::new(3)?;
    let mut call = CallBudget::new("t/call", 20)?;
    assert_eq!(
        queue.submit(&route(&shape, &[])?, &mut call, 10)?,
        Admission::NotNeeded
    );
    assert_eq!(queue.submit(&unknown, &mut call, 10)?, Admission::Enqueued);
    queue.submit(&unknown, &mut CallBudget::new("t/second", 10)?, 10)?;
    queue.submit(&unknown, &mut CallBudget::new("t/third", 10)?, 10)?;
    assert_eq!(
        queue.pop().ok_or(Error::InvalidInput)?.audit_ref(),
        "t/call"
    );
    assert_eq!(
        queue.submit(&unknown, &mut call, 10)?,
        Admission::AlreadySubmitted
    );
    assert_eq!(
        queue.pop().ok_or(Error::InvalidInput)?.audit_ref(),
        "t/second"
    );
    assert_eq!(
        queue.pop().ok_or(Error::InvalidInput)?.audit_ref(),
        "t/third"
    );
    assert!(queue.is_empty());
    Ok(())
}

#[test]
fn maximum_integer_grant_does_not_overflow_on_next_authorization() -> Result<(), Error> {
    let report = report(DriftClass::Unknown);
    let mut queue = Outbox::new(1)?;
    queue.submit(
        &route(&report, &[])?,
        &mut CallBudget::new("t/max", u64::MAX)?,
        u64::MAX,
    )?;
    let mut job = queue.pop().ok_or(Error::InvalidInput)?;
    let grant = job.authorize(u64::MAX)?;
    assert_eq!(job.authorize(1).err(), Some(Error::BudgetExhausted));
    assert_eq!(job.authorized(), u64::MAX);
    let receipt = grant.settle(u64::MAX);
    assert!(!receipt.overrun());
    Ok(())
}
