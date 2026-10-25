//! TI-29 log-sink delivery: one JSON artifact per event, host-driven.
//!
//! A `Sink` is the customer's log sink (NDJSON file, collector shim, log
//! service). Delivery is never awaited by a guarded call and never retried
//! here; the publisher keeps failed events queued and lets the host schedule
//! the next drain.

use crate::contracts::audit_artifact::AuditArtifact;
use std::io::Write;

/// A delivery failure. Never converted into a guard error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SinkError {
    message: String,
    conflict: bool,
}

impl SinkError {
    /// A terminal immutable-ID conflict from the destination.
    pub fn conflict() -> Self {
        Self {
            message: "immutable audit conflict".into(),
            conflict: true,
        }
    }
    pub fn is_conflict(&self) -> bool {
        self.conflict
    }

    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            conflict: false,
        }
    }
}

impl std::fmt::Display for SinkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "audit sink: {}", self.message)
    }
}

impl std::error::Error for SinkError {}

/// One destination for published artifacts.
pub trait Sink {
    /// Tenant-bound destinations expose their scope for the drain fence.
    fn tenant(&self) -> Option<&str> {
        None
    }

    /// Deliver one artifact. A failure leaves the artifact in the outbox.
    fn emit(&mut self, artifact: &AuditArtifact) -> Result<(), SinkError>;
}

/// Streams one compact JSON artifact per line — the customer log-sink shape.
#[derive(Debug)]
pub struct NdjsonSink<W> {
    writer: W,
}

impl<W> NdjsonSink<W> {
    pub fn new(writer: W) -> Self {
        Self { writer }
    }

    /// Recover the wrapped writer once delivery is finished.
    pub fn into_inner(self) -> W {
        self.writer
    }
}

impl<W: Write> Sink for NdjsonSink<W> {
    fn emit(&mut self, artifact: &AuditArtifact) -> Result<(), SinkError> {
        serde_json::to_writer(&mut self.writer, artifact)
            .map_err(|error| SinkError::new(error.to_string()))?;
        self.writer
            .write_all(b"\n")
            .map_err(|error| SinkError::new(error.to_string()))
    }
}
