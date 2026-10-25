//! Cross-language fixture built from real TI-45 admissions and TI-48 projection.
use agw_diff::Side;
use agw_inspector::{Precision, inspect};
use agw_stats::{
    export::{StageLimits, Staging, project},
    windows::{Limits, Projection, Scope, Selection, Store},
};
use chrono::{TimeZone, Utc};
pub fn profiles() -> Result<Vec<serde_json::Value>, Box<dyn std::error::Error>> {
    let start = Utc
        .with_ymd_and_hms(2026, 10, 17, 12, 0, 0)
        .single()
        .ok_or("time")?;
    let end = start + chrono::Duration::seconds(60);
    let mut store = Store::new(Limits {
        samples: 32,
        scopes: 20,
        bytes: 1_000_000,
        batch: 20,
        inspection_nodes: 1000,
        inspection_bytes: 65536,
        minimum: 1,
    })?;
    let scope = Scope {
        tenant: "tenant-a".into(),
        connection: "conn".into(),
        vendor: "payments".into(),
        operation: "GET /charges".into(),
        side: Side::Response,
        field: "/value".into(),
        segment: "retail".into(),
        window: "minute".into(),
        generation: "v1".into(),
    };
    let mut selections = Vec::new();
    for dimension in 0..10 {
        let mut key = scope.clone();
        match dimension {
            1 => key.connection = "other".into(),
            2 => key.tenant = "other".into(),
            3 => key.generation = "v2".into(),
            4 => key.segment = "other".into(),
            5 => key.window = "hour".into(),
            6 => key.side = Side::Request,
            7 => key.vendor = "other".into(),
            8 => key.operation = "GET /other".into(),
            9 => key.field = "/other".into(),
            _ => {}
        }
        selections.push(Selection {
            scope: key,
            projection: Projection::ExactInteger(Precision::Observed),
        });
    }
    for (i, body) in [
        "{}",
        r#"{"value":null}"#,
        r#"{"value":true}"#,
        r#"{"value":"a\u0085\ufeff"}"#,
        r#"{"value":42}"#,
        r#"{"value":9007199254740993}"#,
        r#"{"value":1e400}"#,
    ]
    .iter()
    .enumerate()
    {
        store.record(
            &selections,
            &inspect(body.as_bytes(), false, None),
            start + chrono::Duration::seconds(i as i64),
        )?;
    }
    let mut stage = Staging::new(StageLimits {
        windows: 20,
        bytes: 1_000_000,
    })?;
    for selection in selections {
        stage.close(&mut store, &selection.scope, "close-1", start, end)?;
    }
    let mut output = Vec::new();
    while let Some(closed) = stage.pop() {
        output.push(serde_json::from_slice(&project(&closed, "small")?)?);
    }
    Ok(output)
}
