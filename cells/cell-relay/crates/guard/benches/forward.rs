//! TI-05 — what the forwarding path adds to a typical response.
//!
//! `baseline_copy` moves the same payload through the same `io::copy` without
//! the wrapper, so the difference between it and `typical_payload` *is* the
//! proxy's added latency.
//!
//! Two things to be precise about. Criterion reports the **mean** by default,
//! while EPIC-02's exit criteria are written p50/p99 — TI-04 is what turns this
//! into a gate and has to pick the same statistic. And `TECH_STACK.md` §4's
//! p50 < 50 µs row is the *shape diff*, not this path: the row that budgets a
//! forwarder is **WASM SDK (in-process), < 100 µs added**.

use agw_guard::proxy::{Headers, UpstreamResponse, forward};
use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use std::hint::black_box;
use std::io::{self, Cursor};
use std::time::Duration;

const TYPICAL: &[u8] = include_bytes!("../fixtures/typical_response.json");

fn upstream_headers() -> Headers {
    Headers::from(vec![
        (
            "Content-Type".to_string(),
            "application/json; charset=utf-8".to_string(),
        ),
        ("Content-Length".to_string(), TYPICAL.len().to_string()),
        ("X-Request-Id".to_string(), "req_01HQ".to_string()),
    ])
}

fn bench_forward(c: &mut Criterion) {
    let mut group = c.benchmark_group("forward");

    // Both benches write into a reused Vec and black-box the written bytes.
    // `io::sink()` is elided by the optimizer — a copy with no observable
    // effect measures at ~1.6ns, which is not a copy at all.
    group.throughput(Throughput::Bytes(TYPICAL.len() as u64));
    group.bench_function("baseline_copy", |b| {
        let mut out = Vec::with_capacity(TYPICAL.len());
        b.iter(|| {
            out.clear();
            let mut source = Cursor::new(black_box(TYPICAL));
            let _ = io::copy(&mut source, &mut out);
            black_box(out.as_slice());
        });
    });

    group.bench_function("typical_payload", |b| {
        let mut out = Vec::with_capacity(TYPICAL.len());
        b.iter(|| {
            out.clear();
            let upstream = UpstreamResponse {
                status: 200,
                headers: upstream_headers(),
                body: Cursor::new(black_box(TYPICAL)),
            };
            let mut forwarded = forward(upstream, Duration::from_millis(1));
            let _ = io::copy(&mut forwarded.body, &mut out);
            black_box(out.as_slice());
        });
    });

    group.throughput(Throughput::Elements(1));
    group.bench_function("head_only", |b| {
        b.iter(|| {
            let upstream = UpstreamResponse {
                status: 200,
                headers: upstream_headers(),
                body: Cursor::new(&[][..]),
            };
            let _ = black_box(forward(upstream, Duration::from_millis(1)));
        });
    });

    group.finish();
}

criterion_group!(benches, bench_forward);
criterion_main!(benches);
