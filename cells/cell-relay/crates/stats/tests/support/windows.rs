use agw_diff::Side;
use agw_inspector::{Precision, inspect};
use agw_stats::windows::{Limits, Projection, Scope, Selection, Store};
use chrono::{TimeZone, Utc};

pub fn scope() -> Scope {
    Scope {
        tenant: "tenant-a".into(),
        connection: "connection".into(),
        vendor: "vendor".into(),
        operation: "get".into(),
        side: Side::Response,
        field: "/value".into(),
        segment: "segment".into(),
        window: "hour-1".into(),
        generation: "v1".into(),
    }
}
pub fn selection(scope: Scope) -> Selection {
    Selection {
        scope,
        projection: Projection::ExactInteger(Precision::Observed),
    }
}
pub fn limits() -> Limits {
    Limits {
        samples: 40,
        scopes: 20,
        bytes: 100_000,
        batch: 20,
        inspection_nodes: 1000,
        inspection_bytes: 20_000,
        minimum: 20,
    }
}
pub fn record(
    store: &mut Store,
    selections: &[Selection],
    body: &[u8],
    second: i64,
) -> Result<(), agw_stats::windows::Error> {
    store.record(
        selections,
        &inspect(body, false, None),
        Utc.timestamp_opt(second, 0)
            .single()
            .ok_or(agw_stats::windows::Error::InvalidInput)?,
    )
}
