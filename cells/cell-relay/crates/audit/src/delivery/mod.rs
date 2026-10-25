//! TI-57 native, caller-owned durable delivery. Never invoke on a guard path.
//! One process owns each destination path; mutable ownership serializes drains.
mod metrics;
mod persistence;
mod transport;
use crate::{contracts::audit_artifact::AuditArtifact, sink::Sink};
pub use metrics::DeliveryMetrics;
use persistence::{Entry, Owner, State};
use std::{io, path::Path, time::Instant};
pub use transport::HttpsSender;

/// Bounds include encoded artifact bodies and reserved retry/counter growth.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub entries: usize,
    pub bytes: usize,
    pub attempts: u32,
    pub backoff_ms: u64,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            entries: 1024,
            bytes: 64 * 1024 * 1024,
            attempts: 8,
            backoff_ms: 1000,
        }
    }
}
impl Limits {
    fn validate(self) -> io::Result<()> {
        if self.entries == 0
            || self.entries > 65_536
            || self.bytes < 1024
            || self.bytes > 512 * 1024 * 1024
            || self.attempts == 0
            || self.attempts > 100
            || self.backoff_ms == 0
            || self.backoff_ms > 3_600_000
        {
            return Err(invalid("invalid audit spool limits"));
        }
        Ok(())
    }
}
pub(super) fn invalid(message: &str) -> io::Error {
    io::Error::other(message)
}

/// A single tenant and destination. Assign a separate file per registry/sink.
/// Hosts sharing this value across threads must serialize with their own mutex.
/// After any persistence error reopen before retrying: a rename may have committed.
pub struct Outbox {
    owner: Owner,
    state: State,
    limits: Limits,
    poisoned: bool,
}
impl Outbox {
    pub fn open(path: &Path, tenant: &str, limits: Limits) -> io::Result<Self> {
        limits.validate()?;
        if !crate::valid_identifier(tenant) {
            return Err(invalid("invalid spool tenant"));
        }
        let owner = Owner::acquire(path)?;
        let state = owner.load(tenant, limits)?;
        Ok(Self {
            owner,
            state,
            limits,
            poisoned: false,
        })
    }
    pub fn tenant(&self) -> &str {
        &self.state.tenant
    }
    pub fn len(&self) -> usize {
        self.state.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.state.entries.is_empty()
    }
    pub fn metrics(&self) -> &DeliveryMetrics {
        &self.state.metrics
    }
    fn commit(&mut self, next: State) -> io::Result<()> {
        if self.poisoned {
            return Err(invalid("reopen spool after persistence failure"));
        }
        if let Err(error) = self.owner.save(&next, self.limits.bytes) {
            self.poisoned = true;
            return Err(error);
        }
        self.state = next;
        Ok(())
    }
    /// Durable admission. The caller retains its staging front until success.
    /// Tenant attribution comes from the publisher, never an inferred credential.
    pub fn enqueue(&mut self, tenant: &str, artifact: &AuditArtifact) -> io::Result<()> {
        if self.poisoned {
            return Err(invalid("reopen spool after persistence failure"));
        }
        if tenant != self.tenant() {
            return Err(invalid("spool tenant mismatch"));
        }
        validate(artifact, tenant)?;
        let body =
            String::from_utf8(crate::artifact::canonical(artifact).map_err(io::Error::other)?)
                .map_err(io::Error::other)?;
        let id = crate::text(&artifact.id);
        for entry in &self.state.entries {
            if entry.id == id {
                return if entry.body == body {
                    Ok(())
                } else {
                    Err(invalid("immutable audit conflict"))
                };
            }
        }
        let mut next = self.state.clone();
        next.entries.push(Entry {
            id,
            body,
            attempts: 0,
            next_ms: 0,
        });
        if next.entries.len() > self.limits.entries || next.reserved_bytes()? > self.limits.bytes {
            let mut rejected = self.state.clone();
            rejected.metrics.rejected = rejected.metrics.rejected.saturating_add(1);
            self.commit(rejected)?;
            return Err(invalid("audit spool full"));
        }
        self.commit(next)
    }
    /// At most min(limit, 1024) head operations per tick; failed heads stop a tick.
    /// A reservation is persisted before I/O so restarts cannot reset the budget.
    /// Only an acknowledged head is removed, or an explicitly counted terminal loss.
    pub fn drain(&mut self, now_ms: u64, limit: usize, sink: &mut dyn Sink) -> io::Result<usize> {
        if self.poisoned {
            return Err(invalid("reopen spool after persistence failure"));
        }
        if sink.tenant().is_some_and(|tenant| tenant != self.tenant()) {
            return Err(invalid("destination tenant mismatch"));
        }
        let started = Instant::now();
        let result = self.drain_inner(now_ms, limit.min(1024), sink);
        if !self.poisoned {
            let mut next = self.state.clone();
            next.metrics.drains = next.metrics.drains.saturating_add(1);
            next.metrics.duration_ns = next
                .metrics
                .duration_ns
                .saturating_add(u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX));
            self.commit(next)?;
        }
        result
    }
    fn drain_inner(&mut self, now_ms: u64, limit: usize, sink: &mut dyn Sink) -> io::Result<usize> {
        let mut delivered = 0;
        for _ in 0..limit {
            let Some(entry) = self.state.entries.first() else {
                break;
            };
            if now_ms < entry.next_ms {
                break;
            }
            let mut next = self.state.clone();
            if entry.attempts >= self.limits.attempts {
                next.entries.remove(0);
                next.metrics.drops = next.metrics.drops.saturating_add(1);
                self.commit(next)?;
                continue;
            }
            let artifact: AuditArtifact = serde_json::from_str(&entry.body)?;
            let entry = &mut next.entries[0];
            if entry.attempts > 0 {
                next.metrics.retries = next.metrics.retries.saturating_add(1);
            }
            entry.attempts += 1;
            let delay = self
                .limits
                .backoff_ms
                .saturating_mul(1u64 << entry.attempts.saturating_sub(1).min(63))
                .min(3_600_000);
            entry.next_ms = now_ms.saturating_add(delay);
            self.commit(next)?;
            match sink.emit(&artifact) {
                Ok(()) => {
                    let mut next = self.state.clone();
                    next.entries.remove(0);
                    next.metrics.deliveries = next.metrics.deliveries.saturating_add(1);
                    self.commit(next)?;
                    delivered += 1;
                }
                Err(error) if error.is_conflict() => {
                    let mut next = self.state.clone();
                    next.entries.remove(0);
                    next.metrics.conflicts = next.metrics.conflicts.saturating_add(1);
                    next.metrics.drops = next.metrics.drops.saturating_add(1);
                    self.commit(next)?;
                }
                Err(_) => break,
            }
        }
        Ok(delivered)
    }
}
pub(super) fn validate(artifact: &AuditArtifact, tenant: &str) -> io::Result<()> {
    crate::artifact::validate(artifact).map_err(io::Error::other)?;
    if crate::artifact::byte_len(artifact).map_err(io::Error::other)? > 1_048_576 {
        return Err(invalid("audit artifact exceeds 1 MiB"));
    }
    if artifact
        .metadata
        .get("tenant")
        .is_some_and(|value| value.as_str() != Some(tenant))
    {
        return Err(invalid("artifact tenant mismatch"));
    }
    Ok(())
}
