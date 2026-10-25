#[path = "support/windows.rs"]
mod support;
use agw_inspector::inspect;
use agw_stats::windows::{
    self,
    sampled::{self, Draw, Policy, Selection, Store},
};
use chrono::Utc;
use std::{error::Error, sync::Arc};
fn selected() -> Selection {
    Selection {
        scope: support::scope(),
        array: "/rows".into(),
        item: "/value".into(),
        policy: Policy {
            version: "v1".into(),
            max_items: 2,
        },
        projection: windows::Projection::Disabled,
    }
}
fn record(store: &mut Store, draws: &[Draw<'_>], body: &[u8]) -> Result<(), sampled::Error> {
    store.record(draws, &inspect(body, false, None), Utc::now())
}
#[test]
fn all_preflight_failures_preserve_baseline_identity() -> Result<(), Box<dyn Error>> {
    let mut limits = support::limits();
    limits.samples = 21;
    let mut store = Store::new(limits)?;
    let s = selected();
    let draws = [Draw {
        selection: &s,
        entropy: 0,
    }];
    for _ in 0..20 {
        record(&mut store, &draws, br#"{"rows":[{"value":"old"}]}"#)?;
    }
    store.publish(&s)?;
    for _ in 0..20 {
        record(&mut store, &draws, br#"{"rows":[{"value":"live"}]}"#)?;
    }
    let old = store.snapshot(&s).ready().ok_or("not ready")?;
    let bytes = store.bytes();
    for body in [
        br#"{"rows":[{},{},{}]}"#.as_slice(),
        br#"{"rows":[{}, {"x":1,"x":2}]}"#,
        br#"{"rows":[{"value":[] }]}"#,
    ] {
        assert!(record(&mut store, &draws, body).is_err());
        assert_eq!(store.bytes(), bytes);
    }
    for (array, item) in [
        ("/rows", "/missing/~2"),
        ("/rows", "/value/deeper"),
        ("/rows/0", "/value"),
    ] {
        let mut invalid = s.clone();
        invalid.array = array.into();
        invalid.item = item.into();
        assert!(
            record(
                &mut store,
                &[
                    draws[0],
                    Draw {
                        selection: &invalid,
                        entropy: 0
                    }
                ],
                br#"{"rows":[{"value":1}]}"#
            )
            .is_err()
        );
        assert_eq!(store.bytes(), bytes);
    }
    assert!(record(&mut store, &[draws[0], draws[0]], br#"{"rows":[{}]}"#).is_err());
    let mut incomplete = inspect(br#"{"rows":[{}]}"#, true, None);
    assert!(store.record(&draws, &incomplete, Utc::now()).is_err());
    incomplete = inspect(b"oops", false, None);
    assert!(store.record(&draws, &incomplete, Utc::now()).is_err());
    assert_eq!(store.bytes(), bytes);
    let after = store.snapshot(&s).ready().ok_or("not ready")?;
    assert!(Arc::ptr_eq(old.baseline(), after.baseline()));
    assert!(Arc::ptr_eq(old.live(), after.live()));
    record(&mut store, &draws, br#"{"rows":[{}]}"#)?;
    assert!(record(&mut store, &draws, br#"{"rows":[{}]}"#).is_err());
    Ok(())
}
#[test]
fn staged_scope_byte_sample_and_work_limits_are_atomic() -> Result<(), Box<dyn Error>> {
    let s = selected();
    let mut other = s.clone();
    other.scope.tenant = "other".into();
    let draws = [
        Draw {
            selection: &s,
            entropy: 0,
        },
        Draw {
            selection: &other,
            entropy: 0,
        },
    ];
    let mut baseline = Store::new(support::limits())?;
    record(&mut baseline, &draws[..1], br#"{"rows":[{}]}"#)?;
    let first_bytes = baseline.bytes();
    for kind in 0..5 {
        let mut limits = support::limits();
        match kind {
            0 => limits.bytes = first_bytes + 1,
            1 => {
                limits.scopes = 1;
                limits.batch = 1;
            }
            2 => limits.inspection_nodes = 2,
            3 => limits.inspection_bytes = 3,
            _ => limits.inspection_nodes = 6, // input fits; traversal/extraction does not
        }
        let mut store = Store::new(limits)?;
        assert!(
            record(&mut store, &draws, br#"{"rows":[{}]}"#).is_err(),
            "kind {kind}"
        );
        assert_eq!(store.scopes(), 0);
        assert_eq!(store.bytes(), 0);
    }
    // Exercise the actual scope quota after a first batch fills it.
    let mut limits = support::limits();
    limits.scopes = 1;
    limits.batch = 1;
    let mut store = Store::new(limits)?;
    record(&mut store, &draws[..1], br#"{"rows":[{}]}"#)?;
    let bytes = store.bytes();
    assert!(record(&mut store, &draws[1..], br#"{"rows":[{}]}"#).is_err());
    assert_eq!(store.bytes(), bytes);
    // Existing object-only store still rejects arrays with no opt-in policy.
    let mut plain = windows::Store::new(support::limits())?;
    assert!(
        support::record(
            &mut plain,
            &[support::selection(support::scope())],
            br#"{"value":[]}"#,
            0
        )
        .is_err()
    );
    Ok(())
}
#[test]
fn policy_validation_and_projection_fences() -> Result<(), Box<dyn Error>> {
    let s = selected();
    let mut store = Store::new(support::limits())?;
    for max in [0, 4097] {
        let mut bad = s.clone();
        bad.policy.max_items = max;
        assert!(
            record(
                &mut store,
                &[Draw {
                    selection: &bad,
                    entropy: 0
                }],
                br#"{"rows":[{}]}"#
            )
            .is_err()
        );
    }
    for version in [String::new(), "a".repeat(129)] {
        let mut bad = s.clone();
        bad.policy.version = version;
        assert!(
            record(
                &mut store,
                &[Draw {
                    selection: &bad,
                    entropy: 0
                }],
                br#"{"rows":[{}]}"#
            )
            .is_err()
        );
    }
    record(
        &mut store,
        &[Draw {
            selection: &s,
            entropy: 0,
        }],
        br#"{"rows":[{}]}"#,
    )?;
    let bytes = store.bytes();
    let mut bad = s.clone();
    bad.projection = windows::Projection::ExactInteger(agw_inspector::Precision::Observed);
    assert!(
        record(
            &mut store,
            &[Draw {
                selection: &bad,
                entropy: 0
            }],
            br#"{"rows":[{}]}"#
        )
        .is_err()
    );
    assert_eq!(store.bytes(), bytes);
    Ok(())
}

#[test]
fn unavailable_provider_and_selector_identities_remain_explicit() -> Result<(), Box<dyn Error>> {
    let mut store = Store::new(support::limits())?;
    let s = selected();
    let mut other = s.clone();
    other.array = "/other".into();
    let body = br#"{"rows":[{"value":"a"}],"other":[{"value":"b"}]}"#;
    for selection in [&s, &other] {
        for _ in 0..20 {
            record(
                &mut store,
                &[Draw {
                    selection,
                    entropy: 0,
                }],
                body,
            )?;
        }
        store.publish(selection)?;
        for _ in 0..20 {
            record(
                &mut store,
                &[Draw {
                    selection,
                    entropy: 0,
                }],
                body,
            )?;
        }
    }
    assert_eq!(store.scopes(), 2);
    assert_eq!(
        store
            .snapshot(&s)
            .ready()
            .ok_or("ready")?
            .baseline()
            .vocabulary(),
        vec!["a"]
    );
    assert_eq!(
        store
            .snapshot(&other)
            .ready()
            .ok_or("ready")?
            .baseline()
            .vocabulary(),
        vec!["b"]
    );
    store.set_available(&s, false)?;
    let bytes = store.bytes();
    assert!(
        record(
            &mut store,
            &[Draw {
                selection: &s,
                entropy: 0
            }],
            body
        )
        .is_err()
    );
    assert!(matches!(
        store.snapshot(&s),
        sampled::Availability::Unavailable(_)
    ));
    assert!(store.snapshot(&other).ready().is_some());
    assert_eq!(store.bytes(), bytes);
    store.set_available(&s, true)?;
    assert!(store.snapshot(&s).ready().is_some());
    assert!(store.remove(&s)?);
    assert!(store.remove(&other)?);
    assert_eq!(store.bytes(), 0);
    assert_eq!(store.scopes(), 0);
    Ok(())
}
