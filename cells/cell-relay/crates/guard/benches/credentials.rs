//! TI-61/TI-62: bounded credential hot-path cost, including preallocated history.
use agw_guard::credential::{
    Signal, classify,
    health::Projection,
    tracker::{Observation, Tracker},
};
use agw_guard::proxy::Headers;
use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;
fn benchmark(c: &mut Criterion) {
    let headers = Headers::new();
    let signal = Signal {
        status: 200,
        headers: &headers,
        body: None,
        complete: true,
        available: true,
        revoked: false,
    };
    c.bench_function("credential/classify_success", |b| {
        b.iter(|| black_box(classify(black_box(&signal))))
    });
    let mut tracker = match Tracker::new(4096, 3, 60000) {
        Ok(value) => value,
        Err(error) => panic!("benchmark setup: {error}"),
    };
    let observation = Observation {
        operation: "read",
        arguments: "args-1",
        version: Some("v1"),
        now: 0,
        signal,
    };
    let _ = tracker.observe(&observation);
    c.bench_function("credential/history_success", |b| {
        b.iter(|| black_box(tracker.observe(black_box(&observation))))
    });
    let mut projection = match Projection::new("tenant", 4096, 3, 60000) {
        Ok(value) => value,
        Err(error) => panic!("benchmark setup: {error}"),
    };
    projection.success("tenant", "connection", Some("v1"), 0, 1);
    let mut sequence = 1u64;
    c.bench_function("credential/health_success", |b| {
        b.iter(|| {
            sequence = sequence.saturating_add(1);
            black_box(projection.success("tenant", "connection", Some("v1"), sequence, sequence))
        })
    });
}
criterion_group!(benches, benchmark);
criterion_main!(benches);
