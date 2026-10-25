//! TI-29 publication seam: validate, stamp recurrence, store, enqueue once.
//!
//! Nothing here awaits delivery, and nothing filters by decision — a clean,
//! observed, degraded, bypassed or blocked call all produce an artifact, because
//! a bypass is evidence too. Store refusal is the only outcome that stops a
//! publication; callers may ignore that value (the availability gate does not
//! depend on it).

use crate::Error;
use crate::contracts::audit_artifact::AuditArtifact;
use crate::otel::Metrics;
use crate::recurrence::Recurrence;
use crate::sink::Sink;
use crate::store::{Append, Limits, Store};
use chrono::{DateTime, Utc};
use std::collections::VecDeque;

/// Default bounded outbox depth.
pub const DEFAULT_OUTBOX_CAPACITY: usize = 1024;
/// Hard outbox ceiling.
pub const MAX_OUTBOX_CAPACITY: usize = 65_536;

/// Publication bounds; one config per tenant-bound publisher.
#[derive(Debug, Clone, Copy)]
pub struct PublisherConfig {
    pub limits: Limits,
    pub recurrence_capacity: usize,
    pub outbox_capacity: usize,
}

impl Default for PublisherConfig {
    fn default() -> Self {
        Self {
            limits: Limits::default(),
            recurrence_capacity: crate::recurrence::DEFAULT_CAPACITY,
            outbox_capacity: DEFAULT_OUTBOX_CAPACITY,
        }
    }
}

/// What publication produced. `queued` is false for idempotent repetitions and
/// for a full outbox — the artifact is still stored and retrievable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Publication {
    pub id: String,
    pub outcome: Append,
    pub queued: bool,
}

/// One tenant's local audit pipeline. A host creates one per connection or
/// tenant, drains it on its own schedule, and never calls it from a read path
/// in a way that must complete.
#[derive(Debug)]
pub struct Publisher {
    store: Store,
    recurrence: Recurrence,
    outbox: VecDeque<AuditArtifact>,
    outbox_capacity: usize,
    dropped: u64,
    recurrence_dropped: u64,
    delivered: u64,
    metrics: Metrics,
}

impl Publisher {
    pub fn new(tenant: &str, config: PublisherConfig) -> Result<Self, Error> {
        if config.outbox_capacity == 0 || config.outbox_capacity > MAX_OUTBOX_CAPACITY {
            return Err(Error::Invalid("outbox capacity must be 1..=65536"));
        }
        Ok(Self {
            store: Store::new(tenant, config.limits)?,
            recurrence: Recurrence::new(config.recurrence_capacity)?,
            outbox: VecDeque::new(),
            outbox_capacity: config.outbox_capacity,
            dropped: 0,
            recurrence_dropped: 0,
            delivered: 0,
            metrics: Metrics::default(),
        })
    }

    /// Publish one call's artifact. Idempotent by artifact identity: an
    /// identical repetition is neither re-stamped nor re-delivered.
    ///
    /// Identity is decided on the *received* evidence with the recurrence
    /// stamps this seam owns normalized away, so replaying the unstamped
    /// original against the stamped record is a repetition, not a conflict.
    pub fn publish(
        &mut self,
        mut artifact: AuditArtifact,
        now: DateTime<Utc>,
    ) -> Result<Publication, Error> {
        let id = crate::text(&artifact.id);
        if let Some(stored) = self.store.get(&id, now) {
            return if same_evidence(stored, &artifact)? {
                Ok(Publication {
                    id,
                    outcome: Append::Duplicate,
                    queued: false,
                })
            } else {
                Err(Error::Conflict)
            };
        }
        let mut recurrence_dropped = false;
        let recurrence = if !self.store.contains(&id)
            && let Some(drift) = artifact.drift.as_mut()
        {
            match self.recurrence.prepare(drift, artifact.ts) {
                Ok(pending) => Some(pending),
                Err(_) => {
                    // Recurrence is best-effort enrichment; storage still decides admission.
                    recurrence_dropped = true;
                    None
                }
            }
        } else {
            None
        };
        let queued_artifact = artifact.clone();
        let outcome = self.store.append(artifact, now)?;
        if let Some(pending) = recurrence {
            pending.commit();
        }
        if recurrence_dropped {
            self.recurrence_dropped = self.recurrence_dropped.saturating_add(1);
        }
        if let Some(stored) = self.store.get(&id, now) {
            self.metrics.record(stored);
        }
        let queued = if self.outbox.len() >= self.outbox_capacity {
            self.dropped = self.dropped.saturating_add(1);
            self.metrics.publication_drop();
            false
        } else {
            self.outbox.push_back(queued_artifact);
            true
        };
        Ok(Publication {
            id,
            outcome,
            queued,
        })
    }

    /// Deliver up to `limit` queued artifacts in order. A sink failure keeps the
    /// event at the front and stops the drain; the host retries later. A sink
    /// bound to another tenant is refused before I/O, leaving the queue intact.
    pub fn drain(&mut self, sink: &mut dyn Sink, limit: usize) -> usize {
        if sink
            .tenant()
            .is_some_and(|tenant| tenant != self.store.tenant())
        {
            return 0;
        }
        let mut delivered = 0;
        while delivered < limit {
            let Some(front) = self.outbox.front() else {
                break;
            };
            if sink.emit(front).is_err() {
                break;
            }
            self.outbox.pop_front();
            delivered += 1;
            self.delivered = self.delivered.saturating_add(1);
        }
        delivered
    }

    /// Background-only durable transfer. Disk refusal leaves the current front
    /// queued. This does not count as delivery to the final destination.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn stage(
        &mut self,
        spool: &mut crate::delivery::Outbox,
        limit: usize,
    ) -> std::io::Result<usize> {
        if spool.tenant() != self.store.tenant() {
            return Err(std::io::Error::other("publisher/spool tenant mismatch"));
        }
        let mut staged = 0;
        while staged < limit.min(1024) {
            let Some(front) = self.outbox.front() else {
                break;
            };
            spool.enqueue(self.store.tenant(), front)?;
            self.outbox.pop_front();
            staged += 1;
        }
        Ok(staged)
    }

    /// Queued, not yet delivered artifacts.
    pub fn pending(&self) -> usize {
        self.outbox.len()
    }

    /// Events dropped from a full outbox. They remain stored locally.
    pub fn dropped(&self) -> u64 {
        self.dropped
    }

    /// Drifts published without recurrence stamps because the signature map
    /// was full.
    pub fn recurrence_dropped(&self) -> u64 {
        self.recurrence_dropped
    }

    /// Events delivered through a host drain.
    pub fn delivered(&self) -> u64 {
        self.delivered
    }

    /// Synchronous local retrieval for this publisher's tenant.
    pub fn store(&self) -> &Store {
        &self.store
    }

    /// The OTel projection of every stored publication.
    pub fn metrics(&self) -> &Metrics {
        &self.metrics
    }
}

/// Whether two artifacts carry the same evidence. `first_seen`/`occurrences`
/// are publisher-owned enrichment, not evidence: they never decide identity.
fn same_evidence(stored: &AuditArtifact, received: &AuditArtifact) -> Result<bool, Error> {
    let mut stored = stored.clone();
    let mut received = received.clone();
    for artifact in [&mut stored, &mut received] {
        if let Some(drift) = artifact.drift.as_mut() {
            drift.first_seen = None;
            drift.occurrences = None;
        }
    }
    Ok(crate::artifact::canonical(&stored)? == crate::artifact::canonical(&received)?)
}
