#[path = "support/windows.rs"]
mod support;
use agw_inspector::{Precision, inspect};
use agw_stats::{
    control::Floors,
    windows::{
        self,
        sampled::{self, Assumptions, Draw, Policy, Selection, Store},
    },
};
use chrono::{DateTime, Utc};
use std::error::Error;
fn snapshot(
    a: &[u8],
    b: &[u8],
    count: usize,
    projection: windows::Projection,
) -> Result<sampled::Snapshot, Box<dyn Error>> {
    let mut limits = support::limits();
    limits.samples = 128;
    limits.minimum = count.min(20);
    let mut store = Store::new(limits)?;
    let s = Selection {
        scope: support::scope(),
        array: "/rows".into(),
        item: "/value".into(),
        policy: Policy {
            version: "v1".into(),
            max_items: 10,
        },
        projection,
    };
    let at = DateTime::<Utc>::from_timestamp(42, 0).ok_or("time")?;
    for (index, body) in [a, b].into_iter().enumerate() {
        for _ in 0..count {
            store.record(
                &[Draw {
                    selection: &s,
                    entropy: 0,
                }],
                &inspect(body, false, None),
                at,
            )?;
        }
        if index == 0 {
            store.publish(&s)?;
        }
    }
    store.snapshot(&s).ready().ok_or("not ready".into())
}
fn assumptions() -> Assumptions {
    Assumptions {
        independent_exchangeable_calls: true,
        comparable_populations: true,
    }
}
#[test]
fn unsupported_sampling_assumptions_expose_no_significance() -> Result<(), Box<dyn Error>> {
    let snapshot = snapshot(
        br#"{"rows":[{"value":"old"}]}"#,
        br#"{"rows":[{}]}"#,
        128,
        windows::Projection::Disabled,
    )?;
    for a in [
        Assumptions::default(),
        Assumptions {
            independent_exchangeable_calls: true,
            comparable_populations: false,
        },
        Assumptions {
            independent_exchangeable_calls: false,
            comparable_populations: true,
        },
    ] {
        assert!(matches!(
            snapshot.evidence(a),
            Err(sampled::Error::UnsupportedAssumptions)
        ));
    }
    Ok(())
}
#[test]
fn completeness_keeps_call_denominators_family_and_ti19_gate() -> Result<(), Box<dyn Error>> {
    let snapshot = snapshot(
        br#"{"rows":[{"value":"old"},{"value":"old"}]}"#,
        br#"{"rows":[{},{}]}"#,
        128,
        windows::Projection::Disabled,
    )?;
    let report = snapshot.evidence(assumptions())?.completeness(24)?;
    assert!(report.sufficient());
    assert_eq!(report.tests().len(), 5);
    for test in report.tests() {
        assert_eq!(test.samples(), (128, 128));
        assert_eq!(test.evidence().tests, 24);
    }
    assert!(
        report
            .decision(0, &Floors::new(0.05, 0.2, 0.4, &[])?, "tenant-a")?
            .is_some_and(|d| d.eligible())
    );
    assert!(snapshot.evidence(assumptions())?.completeness(4).is_err());
    // Raw object windows still use their own one-call opportunities.
    let mut plain = windows::Store::new(support::limits())?;
    support::record(
        &mut plain,
        &[support::selection(support::scope())],
        br#"{"value":1}"#,
        42,
    )?;
    Ok(())
}
#[test]
fn enum_counts_and_timestamps_are_real_and_family_includes_known_values()
-> Result<(), Box<dyn Error>> {
    let snapshot = snapshot(
        br#"{"rows":[{"value":"old"}]}"#,
        br#"{"rows":[{"value":"new"}]}"#,
        128,
        windows::Projection::Disabled,
    )?;
    let evidence = snapshot.evidence(assumptions())?;
    let report = evidence.enumeration(24)?;
    assert_eq!(report.tests().len(), 2);
    for test in report.tests() {
        assert_eq!(test.samples(), (128, 128));
        assert_eq!(test.evidence().tests, 24);
    }
    assert_eq!(snapshot.live().enumeration()[0].at.timestamp(), 42);
    assert_eq!(evidence.coercion(24)?.tests().len(), 6);
    assert_eq!(evidence.charset(24)?.tests().len(), 11);
    assert!(evidence.enumeration(1).is_err());
    Ok(())
}
#[test]
fn sampling_never_grants_numeric_projection_or_invents_sample_floors() -> Result<(), Box<dyn Error>>
{
    for projection in [
        windows::Projection::Disabled,
        windows::Projection::ExactInteger(Precision::HighPrecision),
    ] {
        let snapshot = snapshot(
            br#"{"rows":[{"value":1}]}"#,
            br#"{"rows":[{"value":100}]}"#,
            128,
            projection,
        )?;
        assert!(matches!(
            snapshot.evidence(assumptions())?.unit(1),
            Err(sampled::Error::ProjectionUnavailable)
        ));
    }
    let snapshot = snapshot(
        br#"{"rows":[{"value":1}]}"#,
        br#"{"rows":[{"value":100}]}"#,
        128,
        windows::Projection::ExactInteger(Precision::Observed),
    )?;
    let report = snapshot.evidence(assumptions())?.unit(1)?;
    assert_eq!(report.tests()[0].samples(), (128, 128));
    assert!(
        report
            .decision(0, &Floors::new(0.05, 0.2, 0.4, &[])?, "tenant-a")?
            .is_some_and(|d| d.eligible())
    );
    let small = self::snapshot(
        br#"{"rows":[{"value":"old"}]}"#,
        br#"{"rows":[{"value":"new"}]}"#,
        1,
        windows::Projection::Disabled,
    )?;
    let report = small.evidence(assumptions())?.enumeration(2)?;
    assert!(!report.sufficient());
    assert!(report.tests().is_empty());
    Ok(())
}
