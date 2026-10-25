//! TI-52 bounded local projection costs, not an integrated/WASM latency claim.
use agw_inspector::{Number, Precision};
use agw_stats::windows::projection::{self, Budget, Hint, Limits, Mode, Policy, PrecisionSource};
use std::{error::Error, hint::black_box, time::Instant};
fn main() -> Result<(), Box<dyn Error>> {
    let exact = Policy::new(
        Mode::ExactOnly,
        PrecisionSource {
            revision: 1,
            hint: Hint::Absent,
        },
        false,
        Limits::default(),
    )?;
    let approximate = Policy::new(
        Mode::Approximate(Budget::new(1e-12, 1e-15)?),
        exact.precision(),
        false,
        Limits::default(),
    )?;
    let subnormal = Policy::new(
        Mode::Approximate(Budget::new(f64::from_bits(1), 0.)?),
        exact.precision(),
        false,
        Limits::default(),
    )?;
    let long = format!("{}e-1023", num_bigint::BigUint::from(5u8).pow(1023));
    let fixtures = [
        ("exact_half", "0.5", exact),
        ("inexact_tenth", "0.1", approximate),
        ("fractional_money", "34.99", approximate),
        ("subnormal", "5e-324", subnormal),
        ("long_exact_subnormal", long.as_str(), exact),
    ];
    let mut results = Vec::new();
    for (name, token, policy) in fixtures {
        let number = Number::read(token, Precision::Observed);
        let mut times = Vec::with_capacity(2200);
        for i in 0..2400 {
            let start = Instant::now();
            black_box(projection::project(black_box(&number), black_box(policy))?);
            let nanos = start.elapsed().as_nanos();
            if i >= 200 {
                times.push(nanos);
            }
        }
        times.sort_unstable();
        let (p50, p99) = (times[1099], times[2177]);
        println!("{name}: p50={p50} ns p99={p99} ns");
        results.push(serde_json::json!({"fixture":name,"token_bytes":token.len(),"measurements":times.len(),"p50_ns":p50,"p99_ns":p99}));
    }
    let rustc = std::process::Command::new("rustc").arg("-vV").output()?;
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../target/ti52-unit-projection.json");
    std::fs::write(
        path,
        serde_json::to_vec_pretty(&serde_json::json!({
            "scope":"native single-number validation, f64 parsing, exact rational proof, provenance allocation and drop; lossless tree/fixture setup excluded",
            "rustc":String::from_utf8(rustc.stdout)?,"results":results
        }))?,
    )?;
    Ok(())
}
