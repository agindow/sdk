//! TI-06 — the inspection tee's streaming behaviour.
//!
//! Contract §3.1: inspection never alters what the client receives. Contract
//! §4.4: a body the parser cannot read is data to classify, never an error to
//! raise. These tests drive the public surface only, so they fail if the crate
//! stops exposing enough to prove the property.

mod support;

use std::cell::Cell;
use std::io::{self, ErrorKind, Read};
use std::rc::Rc;

use agw_inspector::{DEFAULT_CAP, Tee};
use proptest::prelude::*;
use proptest::test_runner::TestCaseError;

use support::{ChunkedReader, FailingReader, InterruptingReader, SpyReader, read_all};

/// Streams `payload` through a tee and returns the client's bytes, the
/// captured head, and the truncation flag.
fn stream(payload: &[u8], cap: usize, chunk: usize) -> io::Result<(Vec<u8>, Vec<u8>, bool)> {
    let mut tee = Tee::with_cap(ChunkedReader::new(payload, chunk), cap);
    let client = read_all(&mut tee)?;
    Ok((client, tee.captured().to_vec(), tee.truncated()))
}

#[test]
fn forwards_bytes_unchanged_and_captures_a_copy() -> io::Result<()> {
    const PAYLOAD: &[u8] = br#"{"id":"evt_01HQ","amount":1200}"#;
    let (client, captured, truncated) = stream(PAYLOAD, DEFAULT_CAP, 3)?;
    assert_eq!(client, PAYLOAD);
    assert_eq!(captured, PAYLOAD);
    assert!(!truncated);
    Ok(())
}

#[test]
fn default_cap_is_eight_mebibytes() {
    assert_eq!(DEFAULT_CAP, 8 * 1024 * 1024);
}

#[test]
fn a_body_at_the_cap_is_not_truncated() -> io::Result<()> {
    let payload = vec![b'x'; 64];
    let (client, captured, truncated) = stream(&payload, 64, 7)?;
    assert_eq!(client, payload);
    assert_eq!(captured, payload);
    assert!(!truncated, "a body that fits exactly is not truncated");
    Ok(())
}

#[test]
fn a_body_one_byte_over_the_cap_keeps_the_head_and_flags_truncation() -> io::Result<()> {
    let payload = vec![b'x'; 65];
    let (client, captured, truncated) = stream(&payload, 64, 7)?;
    assert_eq!(client, payload, "every byte still reaches the client");
    assert_eq!(captured, payload[..64]);
    assert!(truncated);
    Ok(())
}

#[test]
fn capture_stops_at_the_cap_even_for_a_large_stream() -> io::Result<()> {
    let payload = vec![b'y'; 1024 * 1024];
    let (client, captured, truncated) = stream(&payload, 100, 4096)?;
    assert_eq!(
        client.len(),
        payload.len(),
        "the client still sees all 1MiB"
    );
    assert_eq!(captured.len(), 100, "only the head is retained");
    assert_eq!(captured, payload[..100]);
    assert!(truncated);
    Ok(())
}

#[test]
fn a_zero_cap_captures_nothing_and_still_forwards() -> io::Result<()> {
    let payload = b"anything";
    let (client, captured, truncated) = stream(payload, 0, 2)?;
    assert_eq!(client, payload);
    assert!(captured.is_empty());
    assert!(truncated);
    Ok(())
}

#[test]
fn an_empty_body_is_forwarded_and_not_truncated() -> io::Result<()> {
    let (client, captured, truncated) = stream(b"", 8, 4)?;
    assert!(client.is_empty());
    assert!(captured.is_empty());
    assert!(!truncated);
    Ok(())
}

#[test]
fn upstream_io_errors_reach_the_client_unmodified() -> io::Result<()> {
    let mut tee = Tee::with_cap(FailingReader::after(2), 8);
    match read_all(&mut tee) {
        Ok(bytes) => panic!("upstream failure was swallowed; read {} bytes", bytes.len()),
        Err(error) => assert_eq!(error.kind(), ErrorKind::ConnectionReset),
    }
    Ok(())
}

/// The bug this pins: an upstream that dies mid-body leaves the retained head
/// a *prefix* of a body that never finished — the same fact the cap records,
/// reached by the network instead of by us. Without the flag a classifier sees
/// `Eof` on a short head and reports `malformed_body` (contract §4.4) for a
/// body *we* cut short. The cap here is far above the stream so truncation
/// cannot be the explanation.
#[test]
fn an_upstream_failure_marks_the_capture_as_a_prefix() -> io::Result<()> {
    let mut tee = Tee::with_cap(FailingReader::after(6), 4096);
    assert!(
        read_all(&mut tee).is_err(),
        "the upstream was supposed to fail"
    );
    assert_eq!(
        tee.captured().len(),
        6,
        "the bytes that did arrive are kept"
    );
    assert!(
        tee.truncated(),
        "a body that died mid-stream was never finished"
    );
    Ok(())
}

/// `Interrupted` asks to be retried; it does not say the stream is dead. Marking
/// the capture a prefix on it would report a complete body as unfinished the
/// first time any socket returns one, and `read_to_end` and hyper both retry.
#[test]
fn an_interrupted_read_does_not_mark_the_capture_a_prefix() -> io::Result<()> {
    let mut tee = Tee::with_cap(InterruptingReader::new(b"whole body", 2), 4096);
    let client = read_all(&mut tee)?;
    assert_eq!(client, b"whole body", "the retry delivered the rest");
    assert!(!tee.truncated(), "a retried read is not a truncated body");
    Ok(())
}

/// A tee that read ahead would stall the client's next read waiting for bytes
/// nobody asked for. The client must be the only thing that drives the stream.
#[test]
fn does_not_read_upstream_before_the_client_reads() -> io::Result<()> {
    let reads = Rc::new(Cell::new(0));
    let mut tee = Tee::with_cap(
        SpyReader {
            inner: ChunkedReader::new(b"the client drives this", 4),
            reads: Rc::clone(&reads),
            max_requested: Rc::new(Cell::new(0)),
        },
        1024,
    );
    assert_eq!(reads.get(), 0, "constructing a tee must not touch the body");

    let mut buf = [0u8; 8];
    assert!(
        tee.read(&mut buf)? > 0,
        "the first client read yields bytes"
    );
    assert!(
        reads.get() > 0,
        "the body is read only when the client reads"
    );
    Ok(())
}

/// The load-bearing half of the no-read-ahead property, under a cap small
/// enough that a slurping tee would visibly exceed the client's buffer.
#[test]
fn never_requests_more_than_the_client_buffer() -> io::Result<()> {
    const SIZE: usize = 1024 * 1024;
    let payload = vec![0xA5u8; SIZE];
    let max_requested = Rc::new(Cell::new(0));
    let mut tee = Tee::with_cap(
        SpyReader {
            inner: ChunkedReader::new(&payload, 4096),
            reads: Rc::new(Cell::new(0)),
            max_requested: Rc::clone(&max_requested),
        },
        4096,
    );

    let mut buf = [0u8; 100];
    let mut total = 0usize;
    loop {
        let n = tee.read(&mut buf)?;
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

proptest! {
    /// The load-bearing property: whatever the data, the cap, and the chunking,
    /// the client's bytes are the input bytes and the capture is their head.
    #[test]
    fn any_stream_reaches_the_client_and_the_capture_is_its_head(
        data in proptest::collection::vec(any::<u8>(), 0..=4096),
        cap in 0usize..=4200,
        chunk in 1usize..=16,
    ) {
        let mut tee = Tee::with_cap(ChunkedReader::new(&data, chunk), cap);
        let client = read_all(&mut tee)
            .map_err(|error| TestCaseError::fail(format!("read failed: {error}")))?;
        prop_assert_eq!(client.as_slice(), data.as_slice());
        prop_assert_eq!(tee.captured(), &data[..cap.min(data.len())]);
        prop_assert_eq!(tee.truncated(), data.len() > cap);
    }
}
