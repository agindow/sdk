#[path = "support/windows.rs"]
mod support;
use agw_stats::{
    export::{StageLimits, Staging, project},
    windows::Store,
};
use chrono::{TimeZone, Utc};
use support::{limits, record, scope, selection};

#[test]
fn close_detaches_without_publishing_and_projects_losslessly()
-> Result<(), Box<dyn std::error::Error>> {
    let mut store = Store::new(limits())?;
    let key = scope();
    for (i, body) in [
        r#"{}"#,
        r#"{"value":null}"#,
        r#"{"value":42}"#,
        r#"{"value":9007199254740993}"#,
        r#"{"value":1e400}"#,
        r#"{"value":"abc"}"#,
    ]
    .iter()
    .enumerate()
    {
        record(
            &mut store,
            &[selection(key.clone())],
            body.as_bytes(),
            i as i64,
        )?;
    }
    let mut staging = Staging::new(StageLimits {
        windows: 2,
        bytes: 100_000,
    })?;
    staging.close(
        &mut store,
        &key,
        "close-1",
        Utc.timestamp_opt(0, 0).single().ok_or("time")?,
        Utc.timestamp_opt(10, 0).single().ok_or("time")?,
    )?;
    assert_eq!(staging.len(), 1);
    assert!(store.snapshot(&key).ready().is_none());
    record(
        &mut store,
        &[selection(key.clone())],
        br#"{"value":99}"#,
        11,
    )?;
    let closed = staging.pop().ok_or("closed")?;
    let profile = project(&closed, "small")?;
    let value: serde_json::Value = serde_json::from_slice(&profile)?;
    assert_eq!(value["types"], serde_json::json!([1, 1, 0, 1, 2, 1, 0, 0]));
    assert_eq!(value["excluded_numbers"], 2);
    assert_eq!(
        value["numeric"]
            .as_array()
            .ok_or("bins")?
            .iter()
            .filter_map(|v| v.as_u64())
            .sum::<u64>(),
        1
    );
    assert_eq!(
        value["scope"]["tenant_segment"],
        r#"["ti48","connection","v1","segment","hour-1"]"#
    );
    assert_eq!(profile, project(&closed, "small")?);
    assert_ne!(profile, project(&closed, "large")?);
    Ok(())
}

#[test]
fn full_staging_rejects_new_without_erasing_local_samples() -> Result<(), Box<dyn std::error::Error>>
{
    let mut store = Store::new(limits())?;
    let key = scope();
    let mut staging = Staging::new(StageLimits {
        windows: 1,
        bytes: 100_000,
    })?;
    let start = Utc.timestamp_opt(0, 0).single().ok_or("time")?;
    let end = Utc.timestamp_opt(10, 0).single().ok_or("time")?;
    record(&mut store, &[selection(key.clone())], br#"{"value":1}"#, 1)?;
    staging.close(&mut store, &key, "one", start, end)?;
    record(&mut store, &[selection(key.clone())], br#"{"value":2}"#, 2)?;
    assert!(staging.close(&mut store, &key, "two", start, end).is_err());
    assert_eq!(staging.rejected(), 1);
    staging.pop();
    staging.close(&mut store, &key, "two", start, end)?;
    let value: serde_json::Value =
        serde_json::from_slice(&project(&staging.pop().ok_or("closed")?, "small")?)?;
    assert_eq!(value["types"][4], 1);
    Ok(())
}

#[test]
fn rejected_input_and_bad_intervals_never_create_an_export()
-> Result<(), Box<dyn std::error::Error>> {
    let mut store = Store::new(limits())?;
    let key = scope();
    for body in [r#"{"value":1,"value":2}"#, "{", r#"{"value":[]}"#] {
        assert!(record(&mut store, &[selection(key.clone())], body.as_bytes(), 1).is_err());
    }
    let mut staging = Staging::new(StageLimits {
        windows: 1,
        bytes: 100_000,
    })?;
    let at = Utc.timestamp_opt(1, 0).single().ok_or("time")?;
    assert!(staging.close(&mut store, &key, "empty", at, at).is_err());
    assert!(staging.is_empty());
    Ok(())
}

#[path = "support/export.rs"]
mod fixture;
#[test]
fn real_projection_matches_the_frozen_cross_language_fixture()
-> Result<(), Box<dyn std::error::Error>> {
    let expected: serde_json::Value =
        serde_json::from_str(include_str!("support/relay-profile.json"))?;
    assert_eq!(serde_json::to_value(fixture::profiles()?)?, expected);
    Ok(())
}

#[test]
fn byte_cap_and_invalid_close_do_not_erase_or_publish_windows()
-> Result<(), Box<dyn std::error::Error>> {
    let mut store = Store::new(limits())?;
    let key = scope();
    record(&mut store, &[selection(key.clone())], br#"{"value":5}"#, 5)?;
    let at = Utc.timestamp_opt(5, 0).single().ok_or("time")?;
    let mut stage = Staging::new(StageLimits {
        windows: 1,
        bytes: 1,
    })?;
    assert!(
        stage
            .close(
                &mut store,
                &key,
                "one",
                at,
                at + chrono::Duration::seconds(1)
            )
            .is_err()
    );
    let mut stage = Staging::new(StageLimits {
        windows: 1,
        bytes: 100_000,
    })?;
    assert!(
        stage
            .close(
                &mut store,
                &key,
                "one",
                at - chrono::Duration::seconds(1),
                at
            )
            .is_err()
    );
    stage.close(
        &mut store,
        &key,
        "one",
        at,
        at + chrono::Duration::seconds(1),
    )?;
    assert!(stage.bytes() > 0);
    stage.pop();
    assert_eq!(stage.bytes(), 0);
    Ok(())
}

#[test]
fn oversized_scope_is_rejected_without_silently_collapsing_identity()
-> Result<(), Box<dyn std::error::Error>> {
    let mut store = Store::new(limits())?;
    let mut key = scope();
    key.connection = "x".repeat(256);
    record(&mut store, &[selection(key.clone())], br#"{"value":5}"#, 5)?;
    let at = Utc.timestamp_opt(5, 0).single().ok_or("time")?;
    let mut stage = Staging::new(StageLimits {
        windows: 1,
        bytes: 100_000,
    })?;
    stage.close(
        &mut store,
        &key,
        "one",
        at,
        at + chrono::Duration::seconds(1),
    )?;
    assert!(project(&stage.pop().ok_or("closed")?, "small").is_err());
    Ok(())
}

#[test]
fn synchronous_handoff_has_no_profiler_storage_or_transport_dependencies() {
    for source in [
        include_str!("../src/export/mod.rs"),
        include_str!("../src/windows/store.rs"),
    ] {
        for forbidden in [
            "fs::",
            "std::net",
            "std::process",
            "native::",
            "project(",
            "reqwest",
            "tokio::",
        ] {
            assert!(
                !source.contains(forbidden),
                "synchronous collection references {forbidden}"
            );
        }
    }
}
