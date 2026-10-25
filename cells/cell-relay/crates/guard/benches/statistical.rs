//! Native integrated path: parsing, transient extraction, TI-50, TI-19, TI-20,
//! TI-24 and containment. Setup/network/streaming/WASM are excluded.
#[path = "../tests/support/statistical.rs"]
mod fixtures;
use agw_diff::Side;
use agw_guard::statistical::{Engine, Operation, Plan};
use criterion::{Criterion, criterion_group, criterion_main};
use std::{hint::black_box, time::Duration};

fn run(c: &mut Criterion) -> Result<(), Box<dyn std::error::Error>> {
    let mut group = c.benchmark_group("statistical");
    group
        .sample_size(100)
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(3));
    for (name, baseline, live) in [
        ("unit_both", "1", "100"),
        ("enum_charset_both", "\"old\"", "\"12345\""),
    ] {
        let plan = Plan::new(
            fixtures::identity(),
            vec![
                fixtures::field(Side::Request, baseline, live)?,
                fixtures::field(Side::Response, baseline, live)?,
            ],
        )?;
        let engine = Engine::statistical(plan);
        let context = fixtures::context("observe")?;
        let bytes = format!("{{\"value\":{live}}}");
        group.bench_function(name, |b| {
            b.iter(|| {
                let output = engine
                    .begin_call(context, Operation::Write, "bench", 100_000_000, 0)
                    .dispatch(
                        fixtures::body(black_box(bytes.as_bytes())),
                        None,
                        || black_box(bytes.as_bytes()),
                        |bytes| fixtures::body(bytes),
                    );
                assert_eq!(output.bypasses, 0);
                assert_eq!(
                    output.request.records.len(),
                    if name == "unit_both" { 5 } else { 4 }
                );
                assert_eq!(
                    output.response.as_ref().map(|p| p.records.len()),
                    Some(if name == "unit_both" { 5 } else { 4 })
                );
                let _ = black_box(output);
            })
        });
    }
    // Same 862-byte fixture used by TI-42; one declared /amount field on both
    // phases. Retain whitespace and unrelated fields, including nested arrays.
    let typical = include_bytes!("../fixtures/typical_response.json");
    let mut fields = vec![
        fixtures::field(Side::Request, "1999", "1999")?,
        fixtures::field(Side::Response, "1999", "1999")?,
    ];
    // Keep exact scopes: build snapshots using the selected /amount path.
    for f in &mut fields {
        *f = fixtures::field_path(f.scope.side, "1999", "1999", 128, "/amount")?;
    }
    let engine = Engine::statistical(Plan::new(fixtures::identity(), fields)?);
    let context = fixtures::context("observe")?;
    group.bench_function("typical_862_both", |b| {
        b.iter(|| {
            let output = engine
                .begin_call(context, Operation::Read, "bench", 100_000_000, 0)
                .dispatch(
                    fixtures::body(black_box(typical)),
                    None,
                    || black_box(typical.as_slice()),
                    |bytes| fixtures::body(bytes),
                );
            assert_eq!(output.bypasses, 0);
            assert_eq!(output.request.records.len(), 5);
            assert_eq!(output.response.as_ref().map(|p| p.records.len()), Some(5));
            let _ = black_box(output);
        })
    });
    engine.set_ready(false);
    group.bench_function("configured_not_ready", |b| {
        b.iter(|| {
            let output = engine
                .begin_call(context, Operation::Write, "bench", 0, 0)
                .dispatch(
                    fixtures::body(black_box(typical)),
                    None,
                    || black_box(typical.as_slice()),
                    |_| panic!("bypass must skip response capture"),
                );
            assert_eq!(output.bypasses, 1);
            assert!(output.request.records.is_empty());
            let _ = black_box(output);
        })
    });
    group.finish();
    Ok(())
}
fn benchmark(c: &mut Criterion) {
    if let Err(e) = run(c) {
        panic!("statistical benchmark: {e}")
    }
}
criterion_group!(benches, benchmark);
criterion_main!(benches);
