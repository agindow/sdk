//! TI-17 native fixture p99 gate. Parsing and fixture construction are excluded.
use agw_diff::DriftClass;
use agw_inspector::{Number, Value};
use agw_stats::field::{Report, charset, coercion, completeness};
use serde_json::json;
use std::{error::Error, hint::black_box, time::Instant};

type Detector =
    fn(&str, &[Option<&Value>], &[Option<&Value>]) -> Result<Report, agw_stats::field::Error>;

fn main() -> Result<(), Box<dyn Error>> {
    let number = Value::Number(Number::I64(42));
    let null = Value::Null;
    let text = Value::String("42".into());
    let iso = Value::String("2026-01-05T12:30:00.123Z".into());
    let month = Value::String("Jan 5".into());
    let fixtures: [(&str, Detector, &Value, &Value, &str); 3] = [
        (
            "completeness",
            completeness,
            &number,
            &null,
            "integer → null",
        ),
        ("coercion", coercion, &number, &text, "integer → string"),
        (
            "charset",
            charset,
            &iso,
            &month,
            "ISO timestamp → month/day",
        ),
    ];
    let mut results = Vec::new();
    for (name, detector, a, b, description) in fixtures {
        let (baseline, observed) = (vec![Some(a); 128], vec![Some(b); 128]);
        let run = || {
            detector(
                black_box("field"),
                black_box(&baseline),
                black_box(&observed),
            )
        };
        assert_eq!(run()?.class(), Some(DriftClass::Semantic));
        for _ in 0..2000 {
            black_box(run()?);
        }
        let mut times = Vec::with_capacity(10000);
        for _ in 0..10000 {
            let start = Instant::now();
            black_box(run()?);
            times.push(start.elapsed().as_nanos());
        }
        times.sort_unstable();
        let (p50, p99) = (times[4999], times[9899]);
        println!("{name}: p50={p50} ns, p99={p99} ns; budget <2000000 ns");
        results.push(
            json!({"detector":name,"fixture":description,"samples_per_window":128,
            "measurements":times.len(),"p50_ns":p50,"p99_ns":p99,
            "p99_budget_ns":2000000,"passed":p99<2000000}),
        );
    }
    let rustc = std::process::Command::new("rustc").arg("-vV").output()?;
    let output = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../target/ti17-field-detectors.json");
    std::fs::create_dir_all(output.parent().ok_or("report parent missing")?)?;
    std::fs::write(
        &output,
        serde_json::to_vec_pretty(&json!({
            "timestamp_unix_seconds":std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_secs(),
            "os":std::env::consts::OS,"arch":std::env::consts::ARCH,
            "logical_cpus":std::thread::available_parallelism()?.get(),
            "rustc":String::from_utf8(rustc.stdout)?,"profile":"bench/release",
            "scope":"validation, both-window profiling, comparison and report drop; parsing/setup excluded",
            "quantile_method":"nearest rank","warmup_iterations":2000,"results":results,
        }))?,
    )?;
    println!("Report: {}", output.display());
    if results.iter().any(|r| r["passed"] != true) {
        return Err("TI-17 p99 latency budget exceeded".into());
    }
    Ok(())
}
