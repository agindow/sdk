use agw_stats::control::{AlertAction, AlertBudget, Error};
#[test]
fn reserves_last_slot_and_updates_one_summary_without_flooding() -> Result<(), Error> {
    let mut budget = AlertBudget::new("t", "connection", 3)?;
    assert_eq!(budget.scope(), ("t", "connection"));
    assert_eq!(budget.record(100)?, AlertAction::Individual);
    assert_eq!(budget.record(100)?, AlertAction::Individual);
    assert_eq!(
        budget.record(100)?,
        AlertAction::CreateSummary { consolidated: 1 }
    );
    for consolidated in 2..100 {
        assert_eq!(
            budget.record(100)?,
            AlertAction::UpdateSummary { consolidated }
        );
    }
    assert_eq!(budget.notifications(), 3);
    assert_eq!(budget.record(101)?, AlertAction::Individual);
    assert_eq!(budget.notifications(), 1);
    assert_eq!(budget.record(100), Err(Error::StaleDay));
    assert_eq!(budget.notifications(), 1);
    Ok(())
}
#[test]
fn cap_one_consolidates_immediately_and_zero_sends_nothing() -> Result<(), Error> {
    let mut one = AlertBudget::new("t", "c", 1)?;
    assert_eq!(
        one.record(0)?,
        AlertAction::CreateSummary { consolidated: 1 }
    );
    assert_eq!(
        one.record(0)?,
        AlertAction::UpdateSummary { consolidated: 2 }
    );
    let mut zero = AlertBudget::new("t", "c", 0)?;
    for _ in 0..100 {
        assert_eq!(zero.record(0)?, AlertAction::Disabled);
    }
    assert_eq!(zero.notifications(), 0);
    assert!(AlertBudget::new("", "c", 1).is_err());
    Ok(())
}
