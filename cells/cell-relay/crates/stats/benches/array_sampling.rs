//! TI-51 local native call sampling cost. Parsing and fixture setup excluded.
use agw_diff::Side;
use agw_inspector::inspect;
use agw_stats::windows::{
    Limits, Projection, Scope,
    sampled::{Draw, Error, Policy, Selection, Store},
};
use chrono::{DateTime, Utc};
use std::{error::Error as StdError, hint::black_box, time::Instant};
fn main() -> Result<(), Box<dyn StdError>> {
    let at = DateTime::<Utc>::from_timestamp(0, 0).ok_or("timestamp")?;
    let mut results = Vec::new();
    for length in [0, 1, 1000, 4096] {
        let body = format!(
            "{{\"rows\":[{}]}}",
            vec![r#"{"value":"ok"}"#; length].join(",")
        );
        let inspection = inspect(body.as_bytes(), false, None);
        let selected = Selection {
            scope: Scope {
                tenant: "tenant".into(),
                connection: "connection".into(),
                vendor: "vendor".into(),
                operation: "GET /items".into(),
                side: Side::Response,
                field: "/value".into(),
                segment: "paid".into(),
                window: "hour".into(),
                generation: "v1".into(),
            },
            array: "/rows".into(),
            item: "/value".into(),
            policy: Policy {
                version: "v1".into(),
                max_items: 4096,
            },
            projection: Projection::Disabled,
        };
        let limits = Limits {
            samples: 1,
            scopes: 1,
            bytes: 1_000_000,
            batch: 1,
            inspection_nodes: 40_000,
            inspection_bytes: 1_000_000,
            minimum: 1,
        };
        let mut times = Vec::with_capacity(2200);
        for i in 0..2400 {
            let mut store = Store::new(limits)?;
            let start = Instant::now();
            let result = store.record(
                black_box(&[Draw {
                    selection: &selected,
                    entropy: 4096,
                }]),
                black_box(&inspection),
                at,
            );
            let elapsed = start.elapsed().as_nanos();
            if length == 0 {
                assert!(matches!(result, Err(Error::NoOpportunity)));
            } else {
                result?;
            }
            if i >= 200 {
                times.push(elapsed);
            }
            black_box(store);
        }
        times.sort_unstable();
        let (p50, p99) = (times[1099], times[2177]);
        println!(
            "array_sampling length={length}: p50={p50} ns p99={p99} ns (full validation + one-field admission)"
        );
        results.push(serde_json::json!({"items":length,"measurements":times.len(),"p50_ns":p50,"p99_ns":p99}));
    }
    let output = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../target/ti51-array-sampling.json");
    let rustc = std::process::Command::new("rustc").arg("-vV").output()?;
    std::fs::write(
        output,
        serde_json::to_vec_pretty(&serde_json::json!({
            "scope":"native full complete-input validation, extraction and one-field cold admission; parsing/setup/store creation excluded",
            "rustc":String::from_utf8(rustc.stdout)?,"results":results
        }))?,
    )?;
    Ok(())
}
