//! TI-06 — what the inspection tee costs the response path.
//!
//! Two questions, measured separately because they have different answers.
//!
//! **Forwarding.** `baseline_read` drives the same bytes through the same read
//! loop with no tee, so the gap between it and `forward_and_capture` *is* the
//! tee's added latency. The client pattern is a fixed 64 KiB buffer, which is
//! what an HTTP server actually does — and it keeps the number honest, because
//! nothing in the loop grows with the body.
//!
//! **Parsing.** `inspect_typical_head` is the parse alone, off the client path.
//! It is the cost a classifier pays *after* the bytes are delivered, which is
//! the whole claim: `TECH_STACK.md` §1.2 puts it on a side channel, and this is
//! what that separation is worth.
//!
//! Two things to be precise about. Criterion reports the **mean** by default,
//! while EPIC-02's exit criteria are written p50/p99 — TI-04 is what turns this
//! into a gate and has to pick the same statistic. And `TECH_STACK.md` §4's
//! p50 < 50 µs row is the *shape diff*, not this path: the row that budgets a
//! response path is **WASM SDK (in-process), < 100 µs added**.
//!
//! **Measuring a copy is mostly measuring the compiler.** Two artefacts had to
//! be designed out, and both report absurd *good* numbers, so they are easy to
//! miss: a drain that only counts bytes and never stores them folds away
//! entirely (measured: 509 GiB/s, then 38,963 GiB/s for the 32 MiB case), and a
//! payload filled by `vec![b; n]` is provably uniform, so the loads fold with
//! it. Hence `black_box` on the client's buffer after every drain and a
//! non-uniform large payload.

mod support;

use agw_inspector::{DEFAULT_CAP, Tee, inspect};
use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use std::hint::black_box;
use std::io::{self, Cursor, Read};

/// A request-shaped payload: the dominant write-side case (contract §3.2), and
/// nested enough that a parse has real work to do.
const REQUEST: &[u8] = include_bytes!("../fixtures/typical_request.json");

/// Four times the cap, so the tail of this stream is guaranteed never to be
/// retained.
const LARGE: usize = DEFAULT_CAP * 4;

/// The buffer an HTTP server polls frames into.
const CLIENT_BUFFER: usize = 64 * 1024;

/// Drive a reader the way an HTTP server does: poll into the client's buffer
/// until EOF. The buffer is black-boxed afterwards so the stores those reads
/// make stay observable — see the module note on measurement artefacts.
fn drain<R: Read>(reader: &mut R, buffer: &mut Vec<u8>) -> io::Result<u64> {
    let mut total = 0u64;
    loop {
        let n = reader.read(buffer)?;
        if n == 0 {
            break;
        }
        total += n as u64;
    }
    black_box(buffer.as_slice());
    Ok(total)
}

/// Bytes an HTTP server would actually see: not a repeated constant, because a
/// uniform buffer lets the compiler skip loads it can prove are the same value.
fn payload(size: usize) -> Vec<u8> {
    (0..size).map(|index| (index % 251) as u8).collect()
}

fn bench_typical(c: &mut Criterion) {
    let mut group = c.benchmark_group("tee/typical_payload");
    group.throughput(Throughput::Bytes(REQUEST.len() as u64));
    let mut buffer = vec![0u8; CLIENT_BUFFER];

    group.bench_function("baseline_read", |b| {
        b.iter(|| {
            let mut source = Cursor::new(black_box(REQUEST));
            let _ = black_box(drain(&mut source, &mut buffer));
        });
    });

    group.bench_function("forward_and_capture", |b| {
        b.iter(|| {
            let mut tee = Tee::new(Cursor::new(black_box(REQUEST)));
            let _ = black_box(drain(&mut tee, &mut buffer));
            black_box(tee.captured());
        });
    });

    // Runs after the stream is driven, never inside a read — so this cost is
    // on the side channel by construction, not by convention.
    group.bench_function("inspect_typical_head", |b| {
        b.iter(|| {
            black_box(inspect(black_box(REQUEST), false, None));
        });
    });

    group.finish();
}

/// Past the cap the tee must become a pass-through: the retained head stops
/// growing at 8 MiB while the stream keeps moving. Both benches move the same
/// 32 MiB, so a ratio near 1.0 is the evidence that capture is not a second
/// copy of a large payload.
fn bench_large_body(c: &mut Criterion) {
    let body = payload(LARGE);
    let mut group = c.benchmark_group("tee/large_body_past_cap");
    group.throughput(Throughput::Bytes(LARGE as u64));
    let mut buffer = vec![0u8; CLIENT_BUFFER];

    group.bench_function("baseline_read", |b| {
        b.iter(|| {
            let mut source = Cursor::new(black_box(&body[..]));
            let _ = black_box(drain(&mut source, &mut buffer));
        });
    });

    group.bench_function("forward_and_capture", |b| {
        b.iter(|| {
            let mut tee = Tee::new(Cursor::new(black_box(&body[..])));
            let _ = black_box(drain(&mut tee, &mut buffer));
            black_box(tee.captured());
        });
    });

    for (name, step) in [
        ("doubling_capture", None),
        ("coarse_1m", Some(support::MIB)),
        ("coarse_2m", Some(2 * support::MIB)),
        ("coarse_4m", Some(4 * support::MIB)),
        ("reserve_cap", Some(DEFAULT_CAP)),
    ] {
        group.bench_function(name, |b| {
            b.iter(|| {
                let mut tee = support::Capture::new(Cursor::new(black_box(&body[..])), step);
                let _ = black_box(drain(&mut tee, &mut buffer));
                black_box(tee.captured());
            });
        });
    }

    group.finish();
}

criterion_group!(benches, bench_typical, bench_large_body);
criterion_main!(benches);
