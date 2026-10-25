#[path = "support/windows.rs"]
mod support;
use agw_inspector::{Precision, Value};
use agw_stats::{
    field, unit,
    windows::{
        self, Projection,
        projection::{self, Budget, Hint, Limits, Mode, Policy, PrecisionSource},
    },
};
use std::{error::Error, sync::Arc};
fn policy(mode: Mode) -> Result<Policy, projection::Error> {
    Policy::new(
        mode,
        PrecisionSource {
            revision: 10,
            hint: Hint::Absent,
        },
        false,
        Limits::default(),
    )
}
fn ready(
    baseline: &str,
    observed: &[&str],
    p: Policy,
) -> Result<(windows::Store, windows::Selection), Box<dyn Error>> {
    let mut limits = support::limits();
    limits.minimum = 1;
    let mut store = windows::Store::new(limits)?;
    let mut selected = support::selection(support::scope());
    selected.projection = Projection::Decimal(p);
    support::record(
        &mut store,
        std::slice::from_ref(&selected),
        format!("{{\"value\":{baseline}}}").as_bytes(),
        1,
    )?;
    store.publish(&selected.scope)?;
    for (i, token) in observed.iter().enumerate() {
        support::record(
            &mut store,
            std::slice::from_ref(&selected),
            format!("{{\"value\":{token}}}").as_bytes(),
            2 + i as i64,
        )?;
    }
    Ok((store, selected))
}
#[test]
fn one_refused_number_disables_entire_unit_window_but_keeps_field_evidence()
-> Result<(), Box<dyn Error>> {
    for refused in [
        "0.1",
        "9007199254740992",
        "1e400",
        "1e-400",
        "18446744073709551616",
    ] {
        let (store, selected) = ready(
            "0.5",
            &["0.5", refused, "\"new\"", "null"],
            policy(Mode::ExactOnly)?,
        )?;
        let snapshot = store.snapshot(&selected.scope).ready().ok_or("ready")?;
        assert!(snapshot.live().projected_unit().is_err());
        assert_eq!(snapshot.live().vocabulary(), vec!["new"]);
        assert_eq!(
            field::profile(&snapshot.live().fields())?.opportunities(),
            4
        );
        assert!(
            matches!(snapshot.live().samples()[1].value(), Some(Value::Number(n)) if n.lexeme() == refused)
        );
    }
    let (mut store, s) = ready(
        "0.5",
        &["0.5", "\"new\"", "null", "false"],
        policy(Mode::ExactOnly)?,
    )?;
    support::record(&mut store, std::slice::from_ref(&s), b"{}", 42)?;
    let snapshot = store.snapshot(&s.scope).ready().ok_or("ready")?;
    assert_eq!(
        field::profile(&snapshot.live().fields())?.opportunities(),
        5
    );
    let projected = snapshot.live().projected_unit()?;
    assert_eq!(projected.values(), &[0.5]);
    assert_eq!(projected.samples()[0].index(), 0);
    assert_eq!(projected.samples()[0].at().timestamp(), 2);
    assert_eq!(projected.samples()[0].projection().source(), "0.5");
    // Legacy consumers cannot accidentally discard approximation provenance.
    assert!(snapshot.live().unit().is_none());
    Ok(())
}
#[test]
fn policies_generations_and_immutable_references_never_mix() -> Result<(), Box<dyn Error>> {
    let exact = policy(Mode::ExactOnly)?;
    let (mut store, s) = ready("0.5", &["0.5"], exact)?;
    let before = store.snapshot(&s.scope).ready().ok_or("ready")?;
    for different in [
        policy(Mode::Approximate(Budget::new(1e-12, 0.)?))?,
        policy(Mode::Approximate(Budget::new(0., 1e-12)?))?,
        Policy::new(
            Mode::ExactOnly,
            PrecisionSource {
                revision: 11,
                hint: Hint::Absent,
            },
            false,
            Limits::default(),
        )?,
        Policy::new(
            Mode::ExactOnly,
            PrecisionSource {
                revision: 10,
                hint: Hint::Observed,
            },
            false,
            Limits::default(),
        )?,
    ] {
        let bytes = store.bytes();
        let mut changed = s.clone();
        changed.projection = Projection::Decimal(different);
        assert!(support::record(&mut store, &[changed.clone()], br#"{"value":0.5}"#, 42).is_err());
        assert_eq!(store.bytes(), bytes);
        changed.scope.generation = format!("v{}", store.scopes() + 1);
        support::record(&mut store, &[changed.clone()], br#"{"value":0.5}"#, 42)?;
        store.publish(&changed.scope)?;
        support::record(&mut store, &[changed.clone()], br#"{"value":0.5}"#, 43)?;
        let foreign = store.snapshot(&changed.scope).ready().ok_or("ready")?;
        assert!(matches!(
            projection::compare("/value", before.baseline(), foreign.live()),
            Err(projection::Error::IncompatiblePolicies)
        ));
        // The new generation does not alter the original reference.
        let after = store.snapshot(&s.scope).ready().ok_or("ready")?;
        assert!(Arc::ptr_eq(before.baseline(), after.baseline()));
        assert!(Arc::ptr_eq(before.live(), after.live()));
    }
    Ok(())
}
#[test]
fn fractional_unit_detection_carries_hypothesis_and_projection_provenance()
-> Result<(), Box<dyn Error>> {
    let p = policy(Mode::Approximate(Budget::new(1e-12, 1e-15)?))?;
    let mut limits = support::limits();
    limits.samples = 128;
    let mut store = windows::Store::new(limits)?;
    let mut selected = support::selection(support::scope());
    selected.projection = Projection::Decimal(p);
    for _ in 0..128 {
        support::record(
            &mut store,
            std::slice::from_ref(&selected),
            br#"{"value":3499}"#,
            0,
        )?;
    }
    store.publish(&selected.scope)?;
    for _ in 0..128 {
        support::record(
            &mut store,
            std::slice::from_ref(&selected),
            br#"{"value":34.99}"#,
            1,
        )?;
    }
    let s = store.snapshot(&selected.scope).ready().ok_or("ready")?;
    let report = projection::compare("/value", s.baseline(), s.live())?;
    let unit::Detection::Drift(finding) = report.detection() else {
        return Err("expected drift".into());
    };
    assert_eq!(
        finding.hypothesis().ok_or("hypothesis")?.conversion,
        unit::Conversion::Divide100
    );
    assert!(
        report
            .baseline()
            .samples()
            .iter()
            .all(|x| x.projection().exact())
    );
    assert!(
        report
            .observed()
            .samples()
            .iter()
            .all(|x| !x.projection().exact())
    );
    assert_eq!(report.observed().values().len(), 128);
    assert_eq!(report.observed().policy(), p);
    assert!(
        matches!(s.live().samples()[0].value(), Some(Value::Number(n)) if n.lexeme() == "34.99")
    );
    Ok(())
}
#[test]
fn shared_materialization_budgets_and_store_admission_remain_atomic() -> Result<(), Box<dyn Error>>
{
    let p = policy(Mode::ExactOnly)?;
    let one = projection::project(&agw_inspector::Number::read("0.5", Precision::Observed), p)?;
    assert!(one.exact());
    let limited = Policy::new(
        Mode::ExactOnly,
        PrecisionSource {
            revision: 10,
            hint: Hint::Absent,
        },
        false,
        Limits {
            bytes: 600,
            ..Limits::default()
        },
    )?;
    let (mut store, s) = ready("0.5", &vec!["0.5"; 20], limited)?;
    let old = store.snapshot(&s.scope).ready().ok_or("ready")?;
    assert!(matches!(
        old.live().projected_unit(),
        Err(projection::Error::ResourceLimit)
    ));
    let bytes = store.bytes();
    let mut second = s.clone();
    second.scope.field = "/bad/~2".into();
    assert!(support::record(&mut store, &[s.clone(), second], br#"{"value":0.5}"#, 42).is_err());
    assert_eq!(store.bytes(), bytes);
    let now = store.snapshot(&s.scope).ready().ok_or("ready")?;
    assert!(Arc::ptr_eq(old.baseline(), now.baseline()));
    assert!(Arc::ptr_eq(old.live(), now.live()));
    Ok(())
}

#[test]
fn shared_work_budget_refuses_the_complete_window() -> Result<(), Box<dyn Error>> {
    let p = Policy::new(
        Mode::ExactOnly,
        PrecisionSource {
            revision: 1,
            hint: Hint::Absent,
        },
        false,
        Limits {
            work: 1000,
            ..Limits::default()
        },
    )?;
    assert!(
        projection::project(&agw_inspector::Number::read("0.5", Precision::Observed), p).is_ok()
    );
    let (store, s) = ready("0.5", &vec!["0.5"; 20], p)?;
    let snapshot = store.snapshot(&s.scope).ready().ok_or("ready")?;
    assert!(matches!(
        snapshot.live().projected_unit(),
        Err(projection::Error::ResourceLimit)
    ));
    assert_eq!(snapshot.live().samples().len(), 20);
    Ok(())
}
