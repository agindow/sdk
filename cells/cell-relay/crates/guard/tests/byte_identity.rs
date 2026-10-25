//! TI-05 — byte-for-byte fidelity on the no-repair path.
//!
//! Contract §3.1 names the bug class this suite exists for: a JSON parse and
//! re-serialize destroys a 20-digit integer, normalizes unicode escapes, and
//! rewrites whitespace. The forwarding path must never be the reason any of
//! that happens, so every case here is asserted in bytes, not values.

mod support;

use agw_guard::proxy::{Headers, UpstreamResponse, forward};
use proptest::prelude::*;
use std::cell::Cell;
use std::io::{self, Cursor, Read};
use std::rc::Rc;
use std::time::Duration;

use support::SpyReader;

/// `u64::MAX` (Snowflake territory) — float64 would land on ...551520.
const SNOWFLAKE: &[u8] = br#"{"id":18446744073709551615,"parent":{"id":18446744073709551614}}"#;
/// Raw emoji, a JSON escaped surrogate pair, a combining mark, a ZWJ sequence.
const UNICODE: &[u8] = "{\"emoji\":\"🦀\",\"escaped\":\"\\ud83e\\udd80\",\"combining\":\"e\u{0301}\",\"zwj\":\"👩🏽\u{200d}💻\"}".as_bytes();
/// Tabs, CRLF and a trailing newline are formatting a reserializer would eat.
const WHITESPACE: &[u8] = b"{\n\t\"a\" :\t1,\r\n\t\"b\": [ 2,3 ]\r\n}\n";

fn read_forwarded(body: Vec<u8>, overhead: Duration) -> io::Result<Vec<u8>> {
    let upstream = UpstreamResponse {
        status: 200,
        headers: Headers::new(),
        body: Cursor::new(body),
    };
    let mut forwarded = forward(upstream, overhead);
    let mut out = Vec::new();
    forwarded.body.read_to_end(&mut out)?;
    Ok(out)
}

#[test]
fn preserves_a_twenty_digit_integer_exactly() -> io::Result<()> {
    let out = read_forwarded(SNOWFLAKE.to_vec(), Duration::from_millis(2))?;
    assert_eq!(out, SNOWFLAKE);
    assert!(String::from_utf8_lossy(&out).contains("18446744073709551615"));
    Ok(())
}

#[test]
fn preserves_unicode_sequences_and_escapes_exactly() -> io::Result<()> {
    assert_eq!(read_forwarded(UNICODE.to_vec(), Duration::ZERO)?, UNICODE);
    Ok(())
}

#[test]
fn preserves_whitespace_exactly() -> io::Result<()> {
    assert_eq!(
        read_forwarded(WHITESPACE.to_vec(), Duration::ZERO)?,
        WHITESPACE
    );
    Ok(())
}

/// TI-07 — float precision on the no-repair path.
///
/// Not a new behaviour: TI-05's property test already covers arbitrary bytes, and
/// this is the named case for the numeric half of contract §3.1. It is here
/// because "we don't re-serialize floats" is a claim customers audit for, and a
/// claim with no named assertion eventually loses to a well-meaning refactor.
///
/// `f64::MIN_POSITIVE`, a subnormal, and `1e400` (which is *infinity* in f64) are
/// the three that catch a reserializer: the first two are commonly reformatted,
/// and the third is the one that turns a legal JSON number into a value JSON
/// cannot represent.
#[test]
fn preserves_float_precision_exactly() -> io::Result<()> {
    const FLOATS: &[u8] = concat!(
        r#"{"tiny":2.2250738585072014e-308,"subnormal":5e-324,"overflow":1e400,"#,
        r#""decimals":0.1,"long":1234567890.12345678901234567890,"neg_zero":-0}"#,
    )
    .as_bytes();

    assert_eq!(read_forwarded(FLOATS.to_vec(), Duration::ZERO)?, FLOATS);
    Ok(())
}

/// Yields `remaining` bytes of an ascending 0..=63 block, never allocating it.
struct RepeatingPattern {
    remaining: u64,
    phase: u8,
}

impl Read for RepeatingPattern {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = (buf.len() as u64).min(self.remaining).min(8192) as usize;
        for slot in &mut buf[..n] {
            *slot = self.phase;
            self.phase = (self.phase + 1) % 64;
        }
        self.remaining -= n as u64;
        Ok(n)
    }
}

#[test]
fn streams_fifty_megabytes_without_buffering() -> io::Result<()> {
    const SIZE: u64 = 50 * 1024 * 1024;
    /// 0+1+…+63.
    const BLOCK_SUM: u64 = 2016;

    let max_requested = Rc::new(Cell::new(0));
    let upstream = UpstreamResponse {
        status: 200,
        headers: Headers::new(),
        body: SpyReader {
            inner: RepeatingPattern {
                remaining: SIZE,
                phase: 0,
            },
            reads: Rc::new(Cell::new(0)),
            max_requested: Rc::clone(&max_requested),
        },
    };
    let mut forwarded = forward(upstream, Duration::ZERO);

    let mut buf = vec![0u8; 64 * 1024];
    let mut total = 0u64;
    let mut checksum = 0u64;
    loop {
        let n = forwarded.body.read(&mut buf)?;
        if n == 0 {
            break;
        }
        for byte in &buf[..n] {
            checksum = checksum.wrapping_add(u64::from(*byte));
        }
        total += n as u64;
    }

    assert_eq!(
        total, SIZE,
        "every byte of the 50MB body reaches the client"
    );
    assert_eq!(checksum, (SIZE / 64) * BLOCK_SUM, "no byte was altered");
    assert!(
        max_requested.get() <= buf.len(),
        "read-ahead of {} bytes means the body was buffered",
        max_requested.get()
    );
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 256, ..ProptestConfig::default() })]

    /// Contract §3.1: whatever bytes upstream sent are the bytes the client
    /// receives. Arbitrary bytes are a superset of JSON, so no well-formed
    /// document can fall outside this property.
    #[test]
    fn forwards_arbitrary_bytes_exactly(bytes in proptest::collection::vec(any::<u8>(), 0..=65536)) {
        let expected = bytes.clone();
        let upstream =
            UpstreamResponse { status: 200, headers: Headers::new(), body: Cursor::new(bytes) };
        let mut forwarded = forward(upstream, Duration::from_millis(1));
        let mut out = Vec::new();
        prop_assert!(forwarded.body.read_to_end(&mut out).is_ok());
        prop_assert_eq!(out, expected);
    }
}
