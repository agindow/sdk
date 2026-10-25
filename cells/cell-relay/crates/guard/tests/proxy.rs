//! TI-05 — the forwarding path's public behaviour.
//!
//! Contract §3.1: on the no-repair path the response is a true stream-through.
//! These tests drive the public surface of `agw_guard::proxy` only, so they
//! fail if the crate ever stops exposing enough to prove the property.

mod support;

use agw_guard::proxy::{
    ForwardedResponse, Headers, UpstreamResponse, X_AGINDOW_LATENCY_MS, forward,
};
use std::cell::Cell;
use std::io::{self, Cursor, Read};
use std::rc::Rc;
use std::time::Duration;

use support::SpyReader;

/// A payload shaped like a real tool-call response.
const TYPICAL: &[u8] = include_bytes!("../fixtures/typical_response.json");

fn upstream_headers() -> Headers {
    Headers::from(vec![
        (
            "Content-Type".to_string(),
            "application/json; charset=utf-8".to_string(),
        ),
        ("Content-Length".to_string(), TYPICAL.len().to_string()),
        ("Set-Cookie".to_string(), "a=1; Path=/".to_string()),
        ("Set-Cookie".to_string(), "b=2; Path=/".to_string()),
        ("X-Request-Id".to_string(), "req_01HQ".to_string()),
    ])
}

fn forward_bytes(body: Vec<u8>, overhead: Duration) -> ForwardedResponse<Cursor<Vec<u8>>> {
    let upstream = UpstreamResponse {
        status: 200,
        headers: upstream_headers(),
        body: Cursor::new(body),
    };
    forward(upstream, overhead)
}

fn read_all<R: Read>(reader: &mut R) -> io::Result<Vec<u8>> {
    let mut out = Vec::new();
    reader.read_to_end(&mut out)?;
    Ok(out)
}

/// How many latency headers reached the client, whatever casing they arrived in.
fn latency_header_count(headers: &Headers) -> usize {
    headers
        .iter()
        .filter(|(name, _)| name.eq_ignore_ascii_case(X_AGINDOW_LATENCY_MS))
        .count()
}

#[test]
fn forwards_a_typical_response_body_unchanged() -> io::Result<()> {
    let mut forwarded = forward_bytes(TYPICAL.to_vec(), Duration::from_millis(3));
    assert_eq!(read_all(&mut forwarded.body)?, TYPICAL);
    Ok(())
}

#[test]
fn preserves_upstream_headers_verbatim_and_only_appends_latency() {
    let forwarded = forward_bytes(TYPICAL.to_vec(), Duration::from_millis(3));
    let observed: Vec<(String, String)> = forwarded
        .headers
        .iter()
        .map(|(name, value)| (name.to_string(), value.to_string()))
        .collect();
    let mut expected = vec![
        (
            "Content-Type".to_string(),
            "application/json; charset=utf-8".to_string(),
        ),
        ("Content-Length".to_string(), TYPICAL.len().to_string()),
        // Duplicates survive, in order — a header map could not promise this.
        ("Set-Cookie".to_string(), "a=1; Path=/".to_string()),
        ("Set-Cookie".to_string(), "b=2; Path=/".to_string()),
        ("X-Request-Id".to_string(), "req_01HQ".to_string()),
    ];
    expected.push((X_AGINDOW_LATENCY_MS.to_string(), "3".to_string()));
    assert_eq!(observed, expected);
}

#[test]
fn preserves_the_status_code() {
    let upstream = UpstreamResponse {
        status: 503,
        headers: Headers::new(),
        body: Cursor::new(Vec::new()),
    };
    let forwarded = forward(upstream, Duration::ZERO);
    assert_eq!(forwarded.status, 503);
}

#[test]
fn streams_partial_reads_unchanged() -> io::Result<()> {
    let mut forwarded = forward_bytes(TYPICAL.to_vec(), Duration::ZERO);
    let mut out = Vec::new();
    let mut buf = [0u8; 7];
    loop {
        let n = forwarded.body.read(&mut buf)?;
        if n == 0 {
            break;
        }
        out.extend_from_slice(&buf[..n]);
    }
    assert_eq!(out, TYPICAL);
    Ok(())
}

#[test]
fn does_not_read_upstream_before_the_client_reads() -> io::Result<()> {
    let reads = Rc::new(Cell::new(0));
    let upstream = UpstreamResponse {
        status: 200,
        headers: Headers::new(),
        body: SpyReader {
            inner: Cursor::new(TYPICAL),
            reads: Rc::clone(&reads),
            max_requested: Rc::new(Cell::new(0)),
        },
    };
    let mut forwarded = forward(upstream, Duration::ZERO);
    assert_eq!(reads.get(), 0, "forward must not touch the body");
    let mut buf = [0u8; 64];
    let n = forwarded.body.read(&mut buf)?;
    assert!(n > 0, "the first client read yields bytes");
    assert!(
        reads.get() > 0,
        "the body is read only when the client reads"
    );
    Ok(())
}

#[test]
fn never_requests_more_than_the_client_buffer() -> io::Result<()> {
    const SIZE: usize = 1024 * 1024;
    let reads = Rc::new(Cell::new(0));
    let max_requested = Rc::new(Cell::new(0));
    let upstream = UpstreamResponse {
        status: 200,
        headers: Headers::new(),
        body: SpyReader {
            inner: Cursor::new(vec![0xA5u8; SIZE]),
            reads: Rc::clone(&reads),
            max_requested: Rc::clone(&max_requested),
        },
    };
    let mut forwarded = forward(upstream, Duration::ZERO);
    let mut buf = [0u8; 100];
    let mut total = 0usize;
    loop {
        let n = forwarded.body.read(&mut buf)?;
        if n == 0 {
            break;
        }
        total += n;
    }
    assert_eq!(total, SIZE, "every byte reaches the client");
    assert!(
        max_requested.get() <= buf.len(),
        "read-ahead of {} bytes would buffer the body",
        max_requested.get()
    );
    Ok(())
}

#[test]
fn adds_latency_in_whole_milliseconds() {
    for (elapsed, expected) in [
        (Duration::ZERO, "0"),
        (Duration::from_millis(7), "7"),
        (Duration::from_micros(1999), "1"),
    ] {
        let forwarded = forward_bytes(Vec::new(), elapsed);
        assert_eq!(
            forwarded.headers.get(X_AGINDOW_LATENCY_MS),
            Some(expected),
            "{elapsed:?}"
        );
    }
}

/// Contract §3 makes this header *our* overhead, so a vendor that sends one
/// must not be able to author it — and `get` returns the first match, which
/// would be theirs.
#[test]
fn replaces_an_upstream_latency_header_rather_than_duplicating_it() {
    let mut headers = upstream_headers();
    headers.push(X_AGINDOW_LATENCY_MS, "999");
    let upstream = UpstreamResponse {
        status: 200,
        headers,
        body: Cursor::new(Vec::new()),
    };

    let forwarded = forward(upstream, Duration::from_millis(4));

    assert_eq!(
        latency_header_count(&forwarded.headers),
        1,
        "the vendor's value must not survive beside ours"
    );
    assert_eq!(
        forwarded.headers.get(X_AGINDOW_LATENCY_MS),
        Some("4"),
        "the value is ours"
    );
}

#[test]
fn collapses_repeated_upstream_latency_headers_into_one() {
    let mut headers = upstream_headers();
    headers.push("x-agindow-latency-ms", "1");
    headers.push("X-AGINDOW-LATENCY-MS", "2");
    let upstream = UpstreamResponse {
        status: 200,
        headers,
        body: Cursor::new(Vec::new()),
    };

    let forwarded = forward(upstream, Duration::from_millis(4));

    assert_eq!(
        latency_header_count(&forwarded.headers),
        1,
        "however many copies arrived, one goes out"
    );
    assert_eq!(forwarded.headers.get(X_AGINDOW_LATENCY_MS), Some("4"));
}

#[test]
fn forwards_an_empty_body_with_content_length_preserved() -> io::Result<()> {
    let headers = Headers::from(vec![("Content-Length".to_string(), "0".to_string())]);
    let upstream = UpstreamResponse {
        status: 204,
        headers,
        body: Cursor::new(Vec::new()),
    };
    let mut forwarded = forward(upstream, Duration::from_millis(1));
    assert_eq!(forwarded.headers.get("Content-Length"), Some("0"));
    assert!(read_all(&mut forwarded.body)?.is_empty());
    Ok(())
}

/// `into_inner` is the seam TI-06's tee takes ownership through, so it has to
/// hand back a reader positioned exactly where `forward` left it.
#[test]
fn into_inner_returns_the_original_reader() -> io::Result<()> {
    let forwarded = forward_bytes(TYPICAL.to_vec(), Duration::from_millis(1));

    let mut original = forwarded.body.into_inner();

    assert_eq!(
        read_all(&mut original)?,
        TYPICAL,
        "forward must not consume the body"
    );
    Ok(())
}

#[test]
fn header_lookup_ignores_name_casing() {
    let forwarded = forward_bytes(Vec::new(), Duration::ZERO);
    assert_eq!(
        forwarded.headers.get("content-type"),
        Some("application/json; charset=utf-8")
    );
    assert_eq!(
        forwarded.headers.get("CONTENT-LENGTH"),
        Some(TYPICAL.len().to_string().as_str())
    );
}
