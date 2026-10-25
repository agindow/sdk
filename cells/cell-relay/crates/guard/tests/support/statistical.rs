#![allow(dead_code)]
use agw_diff::Side;
use agw_guard::{
    config::{CallContext, Config, RuntimeConfig},
    statistical::{Field, Identity},
};
use agw_inspector::{Precision, inspect};
use agw_stats::windows::{Limits, Projection, Scope, Selection, Store};
use chrono::{TimeZone, Utc};
type Error = Box<dyn std::error::Error>;
pub fn identity() -> Identity {
    Identity {
        tenant: "tenant".into(),
        connection: "conn".into(),
        vendor: "vendor".into(),
        operation: "endpoint".into(),
        segment: "segment".into(),
    }
}
pub fn context(mode: &str) -> Result<CallContext, Error> {
    Ok(RuntimeConfig::from_sources(
        Config {
            mode: Some(mode.into()),
            ..Config::default()
        },
        None,
    )?
    .begin_call(None)?)
}
pub fn field(side: Side, baseline: &str, live: &str) -> Result<Field, Error> {
    field_samples(side, baseline, live, 128)
}
pub fn field_samples(side: Side, baseline: &str, live: &str, count: usize) -> Result<Field, Error> {
    field_path(side, baseline, live, count, "/value")
}
pub fn field_path(
    side: Side,
    baseline: &str,
    live: &str,
    count: usize,
    path: &str,
) -> Result<Field, Error> {
    field_path_counts(side, baseline, live, count, count, path)
}
pub fn field_counts(
    side: Side,
    baseline: &str,
    live: &str,
    baseline_count: usize,
    live_count: usize,
) -> Result<Field, Error> {
    field_path_counts(side, baseline, live, baseline_count, live_count, "/value")
}
fn field_path_counts(
    side: Side,
    baseline: &str,
    live: &str,
    baseline_count: usize,
    live_count: usize,
    path: &str,
) -> Result<Field, Error> {
    let scope = Scope {
        tenant: "tenant".into(),
        connection: "conn".into(),
        vendor: "vendor".into(),
        operation: "endpoint".into(),
        side,
        field: path.into(),
        segment: "segment".into(),
        window: "selected-hour".into(),
        generation: "v1".into(),
    };
    let selection = Selection {
        scope: scope.clone(),
        projection: Projection::ExactInteger(Precision::Observed),
    };
    let mut store = Store::new(Limits {
        samples: 4096,
        scopes: 4,
        bytes: 2_000_000,
        batch: 4,
        inspection_nodes: 4096,
        inspection_bytes: 65536,
        minimum: 1,
    })?;
    for (index, value) in [baseline, live].into_iter().enumerate() {
        let count = if index == 0 {
            baseline_count
        } else {
            live_count
        };
        let key = path.strip_prefix('/').ok_or("path")?;
        let bytes = format!("{{\"{key}\":{value}}}");
        for at in 0..count {
            store.record(
                std::slice::from_ref(&selection),
                &inspect(bytes.as_bytes(), false, None),
                Utc.timestamp_opt(at as i64, 0)
                    .single()
                    .ok_or("timestamp")?,
            )?;
        }
        if index == 0 {
            store.publish(&scope)?;
        }
    }
    Ok(Field {
        scope: scope.clone(),
        snapshot: store.snapshot(&scope),
        enumeration: true,
    })
}

pub fn body(bytes: &[u8]) -> agw_guard::statistical::Body<'_> {
    agw_guard::statistical::Body::complete(
        bytes,
        chrono::DateTime::UNIX_EPOCH + chrono::Duration::seconds(128),
    )
}
