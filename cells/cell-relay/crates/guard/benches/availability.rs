//! TI-42: actual bypass branches vs the same guard doing local JSON inspection.
use std::hint::black_box;
use std::time::Duration;

use agw_guard::availability::{Budget, EngineFault, RegistrySnapshot, Status};
use agw_guard::config::{Config, RuntimeConfig};
use agw_guard::readiness::AvailabilityGate;
use criterion::{BatchSize, Criterion, criterion_group, criterion_main};

const TYPICAL: &[u8] = include_bytes!("../fixtures/typical_response.json");
type Inspect = fn(&[u8], &(), &mut Budget) -> Result<(), EngineFault>;

fn inspect(input: &[u8], _: &(), budget: &mut Budget) -> Result<(), EngineFault> {
    budget.charge(input.len())?;
    let parsed: serde_json::Value =
        serde_json::from_slice(input).map_err(|_| EngineFault::Internal)?;
    black_box(parsed);
    Ok(())
}

fn fail(_: &[u8], _: &(), _: &mut Budget) -> Result<(), EngineFault> {
    Err(EngineFault::Internal)
}

fn benchmark(c: &mut Criterion) {
    let runtime = match RuntimeConfig::from_sources(Config::default(), None) {
        Ok(runtime) => runtime,
        Err(error) => panic!("benchmark configuration: {error}"),
    };
    let call = match runtime.begin_call(None) {
        Ok(call) => call,
        Err(error) => panic!("benchmark call: {error}"),
    };
    let mut group = c.benchmark_group("availability");
    group
        .sample_size(100)
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(3))
        .confidence_level(0.95)
        .nresamples(10000);
    for name in [
        "not_ready",
        "open_breaker",
        "fresh_error",
        "normal_inspection",
    ] {
        group.bench_function(name, |b| {
            b.iter_batched_ref(
                || {
                    let gate = AvailabilityGate::new(name != "not_ready");
                    if name == "open_breaker" {
                        for _ in 0..3 {
                            black_box(call.guard_with(
                                &gate,
                                TYPICAL,
                                RegistrySnapshot::Ready(&()),
                                TYPICAL.len(),
                                fail,
                                || TYPICAL,
                            ));
                        }
                    }
                    gate
                },
                |gate| {
                    let engine: Inspect = if name == "fresh_error" { fail } else { inspect };
                    let result = black_box(call.guard_with(
                        black_box(gate),
                        black_box(TYPICAL),
                        RegistrySnapshot::Ready(black_box(&())),
                        TYPICAL.len(),
                        black_box(engine),
                        || black_box(TYPICAL),
                    ));
                    let expected = match name {
                        "not_ready" => Status::Bypassed(EngineFault::NotReady),
                        "open_breaker" => Status::Bypassed(EngineFault::CircuitOpen),
                        "fresh_error" => Status::Bypassed(EngineFault::Internal),
                        _ => Status::Inspected,
                    };
                    assert_eq!(result.status, expected, "benchmark measured wrong branch");
                },
                BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

criterion_group!(benches, benchmark);
criterion_main!(benches);
