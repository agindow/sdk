//! A volatile, bounded handoff from an alert producer to a host scheduler.
//!
//! Never invoke this on a guarded call path. [`Emitter::emit`] performs no I/O
//! and never blocks: when the queue is full it refuses with [`Admission::Full`]
//! and counts the refusal. `drain` is host-driven and removes an entry only
//! after the sink accepts it, so a failed head is retried under the same logical
//! notification ID. Durability is the host's commit-before-send concern; this
//! queue claims no crash survival.

use crate::{
    Error,
    envelope::{canonical, validate},
    identifier,
};
use agw_audit::contracts::notification::Notification;
use std::collections::VecDeque;

/// The largest queue one emitter may hold.
pub const MAX_CAPACITY: usize = 4096;
/// The most head operations one `drain` tick performs.
pub const MAX_DRAIN: usize = 1024;

/// A handoff failure. Never converted into a guard error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SinkError {
    message: String,
}

impl SinkError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for SinkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "notification sink: {}", self.message)
    }
}

impl std::error::Error for SinkError {}

/// One host-owned destination for admitted notifications.
pub trait Sink {
    /// Tenant-bound sinks expose their scope for the drain fence.
    fn tenant(&self) -> Option<&str> {
        None
    }

    /// Deliver one notification. A failure leaves it queued.
    fn emit(&mut self, notification: &Notification) -> Result<(), SinkError>;
}

/// What one emission did to the bounded queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Admission {
    Admitted,
    /// The same ID arrived with new summary content; the entry was replaced.
    Updated,
    /// Byte-identical repetition; nothing was queued.
    Duplicate,
    /// The queue is at capacity; the notification was not retained.
    Full,
}

/// Cumulative counters, exported by the host with its own telemetry.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EmitterMetrics {
    pub admitted: u64,
    pub updated: u64,
    pub duplicates: u64,
    pub rejected: u64,
    pub forwarded: u64,
    pub retries: u64,
    pub failures: u64,
}

struct Entry {
    notification: Notification,
    body: Vec<u8>,
    attempts: u32,
}

/// One tenant's bounded handoff queue. A host owns and serializes each value.
pub struct Emitter {
    tenant: String,
    capacity: usize,
    entries: VecDeque<Entry>,
    metrics: EmitterMetrics,
}

impl Emitter {
    pub fn new(tenant: &str, capacity: usize) -> Result<Self, Error> {
        identifier(tenant)?;
        if capacity == 0 || capacity > MAX_CAPACITY {
            return Err(Error::Invalid("invalid notification emitter capacity"));
        }
        Ok(Self {
            tenant: tenant.into(),
            capacity,
            entries: VecDeque::new(),
            metrics: EmitterMetrics::default(),
        })
    }

    pub fn tenant(&self) -> &str {
        &self.tenant
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn metrics(&self) -> EmitterMetrics {
        self.metrics
    }

    /// Validate and hand off one envelope. No I/O, no blocking, no panic.
    pub fn emit(&mut self, notification: Notification) -> Result<Admission, Error> {
        validate(&notification)?;
        if notification.tenant.as_str() != self.tenant {
            return Err(Error::Invalid("notification tenant mismatch"));
        }
        let body = canonical(&notification)?;
        if let Some(index) = self
            .entries
            .iter()
            .position(|entry| entry.notification.id == notification.id)
        {
            let existing = self
                .entries
                .get(index)
                .ok_or(Error::Invalid("invalid emitter entry"))?;
            if existing.body == body {
                self.metrics.duplicates = self.metrics.duplicates.saturating_add(1);
                return Ok(Admission::Duplicate);
            }
            let entry = self
                .entries
                .get_mut(index)
                .ok_or(Error::Invalid("invalid emitter entry"))?;
            entry.notification = notification;
            entry.body = body;
            self.metrics.updated = self.metrics.updated.saturating_add(1);
            return Ok(Admission::Updated);
        }
        if self.entries.len() >= self.capacity {
            self.metrics.rejected = self.metrics.rejected.saturating_add(1);
            return Ok(Admission::Full);
        }
        self.entries.push_back(Entry {
            notification,
            body,
            attempts: 0,
        });
        self.metrics.admitted = self.metrics.admitted.saturating_add(1);
        Ok(Admission::Admitted)
    }

    /// Forward at most `limit` heads in order. A failed head stops the tick and
    /// stays queued; only an accepted entry is removed.
    pub fn drain(&mut self, limit: usize, sink: &mut dyn Sink) -> Result<usize, Error> {
        if sink.tenant().is_some_and(|tenant| tenant != self.tenant) {
            return Err(Error::Invalid("notification sink tenant mismatch"));
        }
        let mut forwarded = 0;
        for _ in 0..limit.min(MAX_DRAIN) {
            let Some(entry) = self.entries.front_mut() else {
                break;
            };
            if entry.attempts > 0 {
                self.metrics.retries = self.metrics.retries.saturating_add(1);
            }
            entry.attempts = entry.attempts.saturating_add(1);
            if sink.emit(&entry.notification).is_err() {
                self.metrics.failures = self.metrics.failures.saturating_add(1);
                break;
            }
            self.entries.pop_front();
            self.metrics.forwarded = self.metrics.forwarded.saturating_add(1);
            forwarded += 1;
        }
        Ok(forwarded)
    }
}
