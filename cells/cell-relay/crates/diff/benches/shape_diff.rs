//! TI-11 native quantile gate: cargo bench -p agw-diff --bench shape_diff.
use agw_diff::{DriftClass, Evidence, Node, Side, Snapshot, compare_inspection};
use agw_inspector::{inference::JsonType as T, inspect};
use serde_json::json;
use std::{error::Error, hint::black_box, time::Instant};

fn baseline() -> Snapshot {
    Snapshot {
        evidence: Evidence::Authoritative,
        node: Node::object([
            (
                "payment".into(),
                Node::object([
                    ("id".into(), Node::scalar(T::String)),
                    ("amount".into(), Node::scalar(T::Integer)),
                    ("currency".into(), Node::scalar(T::String)),
                ]),
            ),
            (
                "items".into(),
                Node::array(Node::object([
                    ("sku".into(), Node::scalar(T::String)),
                    ("quantity".into(), Node::scalar(T::Integer)),
                    ("price".into(), Node::scalar(T::Number)),
                ])),
            ),
        ]),
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let baseline = baseline();
    let fixture = json!({"payment":{"id":"pay_123", "amount":1000, "currency":"USD"},
        "items":(0..20).map(|i| json!({"sku":format!("sku_{i}"), "quantity":2, "price":12.5})).collect::<Vec<_>>()});
    let unchanged = serde_json::to_vec(&fixture)?;
    let mut changed = fixture.clone();
    changed["payment"]["amount"] = json!("1000");
    changed["payment"]["payment_method"] = json!("card_123");
    let changed = serde_json::to_vec(&changed)?;
    let mut results = Vec::new();
    for (name, bytes, expected) in [
        ("unchanged", unchanged.as_slice(), None),
        ("changed", changed.as_slice(), Some(DriftClass::Semantic)),
    ] {
        let inspection = inspect(bytes, false, None);
        let run =
            || compare_inspection(Side::Request, black_box(&baseline), black_box(&inspection));
        assert_eq!(run().class(), expected);
        for _ in 0..2000 {
            black_box(run());
        }
        let mut timings = Vec::with_capacity(20000);
        for _ in 0..20000 {
            let start = Instant::now();
            let result = run();
            // Include the complete returned report and its destruction.
            black_box(result);
            timings.push(start.elapsed().as_nanos());
        }
        timings.sort_unstable();
        let p50 = timings[9999];
        let p99 = timings[19799];
        println!(
            "{name}: p50={p50} ns, p99={p99} ns ({} bytes, 20000 samples)",
            bytes.len()
        );
        results.push(
            json!({"case":name, "bytes":bytes.len(), "samples":timings.len(),
            "p50_ns":p50, "p99_ns":p99, "p50_budget_ns":50000, "p99_budget_ns":500000,
            "passed":p50 < 50000 && p99 < 500000}),
        );
    }
    let rustc = std::process::Command::new("rustc").arg("-vV").output()?;
    let output = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../target/ti11-shape-diff.json");
    std::fs::create_dir_all(output.parent().ok_or("missing report directory")?)?;
    std::fs::write(
        &output,
        serde_json::to_vec_pretty(&json!({
            "timestamp_unix_seconds":std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_secs(),
            "os":std::env::consts::OS, "arch":std::env::consts::ARCH,
            "logical_cpus":std::thread::available_parallelism()?.get(),
            "rustc":String::from_utf8(rustc.stdout)?, "profile":"bench/release", "warmup_iterations":2000,
            "scope":"inspection shape conversion, validation, diff and report drop; parsing/setup excluded",
            "quantile_method":"nearest rank", "results":results,
        }))?,
    )?;
    println!("Report: {}", output.display());
    if results.iter().any(|r| r["passed"] != true) {
        return Err("TI-11 latency budget exceeded".into());
    }
    Ok(())
}
