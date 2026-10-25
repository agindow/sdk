//! The streaming forwarding path — contract §3, TI-05.
//!
//! Upstream bytes reach the client untouched: no parse, no re-serialize, no
//! buffering. The only things this module adds are the verbatim response head
//! and one `X-Agindow-Latency-Ms` header — which is *ours* to write, so an
//! inbound copy is replaced rather than passed on (see [`Headers::claim`]).
//! Inspection (TI-06) and repair (EPIC-03) build on this path; neither may
//! re-encode the bytes that flow through it.

use std::io::{self, Read};
use std::time::Duration;

/// The latency header from contract §3 — `X-Agindow-Latency-Ms`.
pub const X_AGINDOW_LATENCY_MS: &str = "X-Agindow-Latency-Ms";

/// An ordered, case-preserving header list.
///
/// "Preserved verbatim" (TI-05) means a caller observes the same sequence of
/// names and values upstream sent — including duplicates and name casing — so
/// a map that lowercases or reorders would not do.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Headers(Vec<(String, String)>);

impl Headers {
    pub fn new() -> Self {
        Self(Vec::new())
    }

    pub fn push(&mut self, name: impl Into<String>, value: impl Into<String>) {
        self.0.push((name.into(), value.into()));
    }

    /// Claim `name` for AGINDOW: drop every inbound copy (any casing) and append
    /// ours last, so exactly one survives and it is the one we wrote.
    ///
    /// Appending instead would leave the vendor's value on the wire *and* ahead
    /// of ours in [`Headers::get`], which returns the first match — forwarded
    /// instrumentation a vendor can author. See TI-05, "AGINDOW-owned headers
    /// are ours, never the vendor's".
    pub fn claim(&mut self, name: &str, value: impl Into<String>) {
        self.0
            .retain(|(existing, _)| !existing.eq_ignore_ascii_case(name));
        self.push(name, value);
    }

    /// First value for `name`, compared case-insensitively (RFC 9110 §5.1).
    pub fn get(&self, name: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> + '_ {
        self.0
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str()))
    }
}

impl From<Vec<(String, String)>> for Headers {
    fn from(pairs: Vec<(String, String)>) -> Self {
        Self(pairs)
    }
}

/// A response as received from upstream, before forwarding.
#[derive(Debug)]
pub struct UpstreamResponse<B> {
    pub status: u16,
    pub headers: Headers,
    /// The upstream body, touched only as the client consumes it.
    pub body: B,
}

/// The response to emit to the client.
#[derive(Debug)]
pub struct ForwardedResponse<B> {
    pub status: u16,
    /// Upstream headers verbatim, with `X-Agindow-Latency-Ms` appended last and
    /// guaranteed to be AGINDOW's value rather than the vendor's.
    pub headers: Headers,
    pub body: PassthroughBody<B>,
}

/// A body that yields the upstream bytes unchanged.
///
/// Every `read` delegates straight to the upstream reader: nothing is buffered
/// and nothing is read ahead, so a body larger than memory streams through.
#[derive(Debug)]
pub struct PassthroughBody<R> {
    inner: R,
}

impl<R> PassthroughBody<R> {
    pub fn new(inner: R) -> Self {
        Self { inner }
    }

    /// Give the upstream reader back, untouched and unread — the seam TI-06's
    /// inspection tee takes ownership through.
    pub fn into_inner(self) -> R {
        self.inner
    }
}

impl<R: Read> Read for PassthroughBody<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.inner.read(buf)
    }
}

/// Forward an upstream response to the client, byte-for-byte (contract §3.1).
///
/// Status and headers pass through verbatim; `overhead` — AGINDOW's own cost,
/// not the round trip (contract §3) — is recorded as `X-Agindow-Latency-Ms`,
/// truncated to whole milliseconds. It is the only header this function writes,
/// and [`Headers::claim`] makes it ours rather than the vendor's. Separating
/// that overhead from network RTT is the hosted layer's problem (TI-34).
///
/// The body is never parsed, buffered, or re-encoded: the caller drives the
/// reads and the bytes come straight from upstream.
pub fn forward<B: Read>(upstream: UpstreamResponse<B>, overhead: Duration) -> ForwardedResponse<B> {
    let UpstreamResponse {
        status,
        mut headers,
        body,
    } = upstream;
    headers.claim(X_AGINDOW_LATENCY_MS, overhead.as_millis().to_string());
    ForwardedResponse {
        status,
        headers,
        body: PassthroughBody::new(body),
    }
}
