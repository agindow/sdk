//! TI-22 local native p99 gate: both phases, all six forms, audit drafts included.
#[path = "../tests/support/cross_endpoint.rs"]
mod cross;
#[path = "../tests/support/invariants.rs"]
mod fixture;
#[path = "../tests/support/statistical.rs"]
mod stats;
use agw_guard::statistical::{Decision, Engine, Operation, Plan};
use fixture::{BAD, CLEAN, NOVEL};
use serde_json::json;
use std::{hint::black_box, time::Instant};
fn main() -> Result<(), fixture::Error> {
    let engine = fixture::engine()?;
    let mut results = Vec::new();
    for (name, mode, operation, request, response, expected) in [
        (
            "clean_both",
            "repair",
            Operation::Write,
            CLEAN,
            CLEAN,
            Decision::Pass,
        ),
        (
            "violation_both",
            "repair",
            Operation::Read,
            BAD,
            BAD,
            Decision::Degraded,
        ),
        (
            "enum_both",
            "observe",
            Operation::Write,
            NOVEL,
            NOVEL,
            Decision::Pass,
        ),
        (
            "response_halt",
            "repair",
            Operation::Write,
            CLEAN,
            BAD,
            Decision::Halt,
        ),
    ] {
        let context = stats::context(mode)?;
        let run = || {
            engine
                .begin_call(context, operation, "bench", 100_000_000, 0)
                .dispatch(
                    stats::body(black_box(request)),
                    None,
                    || black_box(response),
                    |b| stats::body(b),
                )
        };
        let output = run();
        assert_eq!(output.bypasses, 0);
        assert_eq!(output.request.invariants.len(), 6);
        assert_eq!(
            output.response.as_ref().map(|p| p.invariants.len()),
            Some(6)
        );
        assert_eq!(output.response.as_ref().map(|p| p.decision), Some(expected));
        assert!(output.request.invariant_audit.is_some());
        assert!(
            output
                .response
                .as_ref()
                .is_some_and(|p| p.invariant_audit.is_some())
        );
        for _ in 0..2000 {
            let _ = black_box(run());
        }
        let mut times = Vec::with_capacity(10000);
        for _ in 0..10000 {
            let start = Instant::now();
            let _ = black_box(run());
            times.push(start.elapsed().as_nanos());
        }
        times.sort_unstable();
        let (p50, p99) = (times[4999], times[9899]);
        println!("{name}: p50={p50} ns, p99={p99} ns; budget <100000 ns");
        results.push(json!({"fixture":name, "measurements":times.len(), "p50_ns":p50, "p99_ns":p99,"p99_budget_ns":100000,"passed":p99<100000}));
    }
    let engine = Engine::statistical(
        Plan::new(fixture::identity(), vec![])?
            .with_invariants(agw_diff::Side::Request, fixture::selection()?)?
            .with_invariants(agw_diff::Side::Response, fixture::selection()?)?
            .with_cross_endpoint(cross::selection()?)?,
    );
    let snapshot = cross::snapshot(Some(100), 8000)?;
    let context = stats::context("repair")?;
    let run = || {
        engine
            .begin_call(context, Operation::Write, "bench", 100_000_000, 0)
            .with_workflow(Ok(snapshot.clone()), 2)
            .dispatch(
                stats::body(black_box(CLEAN)),
                None,
                || black_box(CLEAN),
                |b| stats::body(b),
            )
    };
    let checked = run();
    assert_eq!(checked.bypasses, 0);
    assert_eq!(checked.request.invariants.len(), 7);
    assert_eq!(
        checked.request.cross_endpoint[0].reason,
        agw_invariants::cross_endpoint::Reason::Pass
    );
    assert_eq!(
        checked.response.as_ref().map(|p| p.invariants.len()),
        Some(6)
    );
    for _ in 0..2000 {
        let _ = black_box(run());
    }
    let mut times = Vec::with_capacity(10000);
    for _ in 0..10000 {
        let start = Instant::now();
        let _ = black_box(run());
        times.push(start.elapsed().as_nanos());
    }
    times.sort_unstable();
    let (p50, p99) = (times[4999], times[9899]);
    println!("cross_endpoint: p50={p50} ns, p99={p99} ns; budget <100000 ns");
    results.push(json!({"fixture":"cross_endpoint", "measurements":times.len(),"p50_ns":p50,"p99_ns":p99,"p99_budget_ns":100000,"passed":p99<100000}));
    let rustc = std::process::Command::new("rustc").arg("-vV").output()?;
    let output = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../target/ti22-invariants.json");
    std::fs::create_dir_all(output.parent().ok_or("parent")?)?;
    std::fs::write(
        &output,
        serde_json::to_vec_pretty(&json!({
            "timestamp_unix_seconds":std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_secs(),
            "os":std::env::consts::OS,"arch":std::env::consts::ARCH,"logical_cpus":std::thread::available_parallelism()?.get(),
            "rustc":String::from_utf8(rustc.stdout)?,"profile":"bench/release","local_rules_per_phase":6,"cross_endpoint_rules":1,
            "scope":"both-phase parsing, validation, exact extraction, full six-form evaluation, routing/policy, generated audit drafts, headers, containment and result drop; store acquisition/setup/network excluded; cross fixture includes frozen snapshot clone, projection, correlation/freshness, observations and audit",
            "quantile_method":"nearest rank","warmup_iterations":2000,"results":results,
        }))?,
    )?;
    println!("Report: {}", output.display());
    if results.iter().any(|r| r["passed"] != true) {
        return Err("TI-22 p99 latency budget exceeded".into());
    }
    Ok(())
}
