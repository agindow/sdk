use super::{Error, Selected, Snapshot};
use std::{collections::BTreeMap, sync::Arc};
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub capacity: usize,
    pub bytes: usize,
    pub ttl_ms: u64,
    pub refresh_ms: u64,
    pub timeout_ms: u64,
    pub backoff_ms: u64,
    pub recovery_ms: u64,
    pub attempts: u32,
}
#[derive(Debug, Default)]
pub struct Metrics {
    pub attempts: u64,
    pub failures: u64,
    pub installs: u64,
    pub rejections: u64,
    pub conflicts: u64,
    pub missing_reads: u64,
    pub stale_reads: u64,
}
#[derive(Debug)]
pub struct Telemetry {
    pub age_ms: Option<u64>,
    pub next_due_ms: u64,
    pub in_flight: bool,
    pub failures: u32,
}
/// Detached background reservation. No transport is stored in Cache.
#[derive(Debug)]
pub struct Job {
    id: u64,
    nonce: u64,
    deadline: u64,
    pub selected: Selected,
    pub timeout_ms: u64,
}
impl Job {
    pub fn body(&self) -> Result<Vec<u8>, Error> {
        serde_json::to_vec(&self.selected).map_err(|_| Error::Invalid)
    }
}
struct Entry {
    selected: Selected,
    snapshot: Option<Arc<Snapshot>>,
    fresh: Option<u64>,
    due: u64,
    failures: u32,
    flight: Option<(u64, u64)>,
    bytes: usize,
}
pub struct Cache {
    limits: Limits,
    entries: BTreeMap<u64, Entry>,
    sequence: u64,
    now: u64,
    metrics: Metrics,
}
impl Cache {
    pub fn new(limits: Limits) -> Result<Self, Error> {
        if limits.capacity == 0
            || limits.capacity > 4096
            || limits.bytes == 0
            || limits.bytes > 64 * 1024 * 1024
            || limits.ttl_ms == 0
            || limits.ttl_ms > 3_600_000
            || limits.refresh_ms == 0
            || limits.refresh_ms >= limits.ttl_ms
            || limits.timeout_ms == 0
            || limits.timeout_ms > 30_000
            || limits.backoff_ms == 0
            || limits.backoff_ms > 3_600_000
            || limits.recovery_ms == 0
            || limits.recovery_ms > 3_600_000
            || limits.attempts == 0
            || limits.attempts > 16
        {
            return Err(Error::Invalid);
        }
        Ok(Self {
            limits,
            entries: BTreeMap::new(),
            sequence: 0,
            now: 0,
            metrics: Metrics::default(),
        })
    }
    fn clock(&mut self, now: u64) -> u64 {
        self.now = self.now.max(now);
        self.now
    }
    fn next(&mut self) -> Result<u64, Error> {
        self.sequence = self.sequence.checked_add(1).ok_or(Error::Capacity)?;
        Ok(self.sequence)
    }
    fn used(&self) -> usize {
        self.entries.values().map(|e| e.bytes).sum()
    }
    pub fn select(&mut self, selected: Selected, now: u64) -> Result<u64, Error> {
        selected.validate()?;
        let bytes = serde_json::to_vec(&selected)
            .map_err(|_| Error::Invalid)?
            .len()
            + std::mem::size_of::<Entry>();
        if self.entries.len() >= self.limits.capacity
            || self.used().saturating_add(bytes) > self.limits.bytes
            || self.entries.values().any(|e| e.selected == selected)
        {
            return Err(Error::Capacity);
        }
        let id = self.next()?;
        let now = self.clock(now);
        self.entries.insert(
            id,
            Entry {
                selected,
                snapshot: None,
                fresh: None,
                due: now,
                failures: 0,
                flight: None,
                bytes,
            },
        );
        Ok(id)
    }
    /// Allocate a fresh epoch before removal; failure leaves the old selection intact.
    pub fn replace(&mut self, id: u64, selected: Selected, now: u64) -> Result<u64, Error> {
        selected.validate()?;
        if self.entries.get(&id).ok_or(Error::Obsolete)?.selected == selected {
            self.notify(id, now)?;
            return Ok(id);
        }
        let old = self.entries.remove(&id).ok_or(Error::Obsolete)?;
        match self.select(selected, now) {
            Ok(new) => Ok(new),
            Err(error) => {
                self.entries.insert(id, old);
                Err(error)
            }
        }
    }
    /// Local-only, one bounded map lookup and Arc clone. Never invokes a loader.
    pub fn read(&mut self, id: u64, now: u64) -> Option<Arc<Snapshot>> {
        let now = self.clock(now);
        if let Some(entry) = self.entries.get(&id)
            && let Some(fresh) = entry.fresh
        {
            if now.saturating_sub(fresh) < self.limits.ttl_ms {
                return entry.snapshot.clone();
            }
            self.metrics.stale_reads = self.metrics.stale_reads.saturating_add(1);
            return None;
        }
        self.metrics.missing_reads = self.metrics.missing_reads.saturating_add(1);
        None
    }
    pub fn metrics(&self) -> &Metrics {
        &self.metrics
    }
    pub fn telemetry(&self, id: u64, now: u64) -> Option<Telemetry> {
        self.entries.get(&id).map(|e| Telemetry {
            age_ms: e.fresh.map(|f| now.max(self.now).saturating_sub(f)),
            next_due_ms: e.due,
            in_flight: e.flight.is_some(),
            failures: e.failures,
        })
    }
    pub fn notify(&mut self, id: u64, now: u64) -> Result<(), Error> {
        let now = self.clock(now);
        let entry = self.entries.get_mut(&id).ok_or(Error::Obsolete)?;
        if entry.failures == 0 && entry.flight.is_none() {
            entry.due = entry.due.min(now);
        }
        Ok(())
    }
    fn failed(&mut self, id: u64, now: u64) {
        if let Some(e) = self.entries.get_mut(&id) {
            e.flight = None;
            e.failures = e.failures.saturating_add(1);
            let delay = if e.failures >= self.limits.attempts {
                self.limits.recovery_ms
            } else {
                self.limits
                    .backoff_ms
                    .saturating_mul(1_u64 << (e.failures - 1))
                    .min(3_600_000)
            };
            e.due = now.saturating_add(delay);
            self.metrics.failures = self.metrics.failures.saturating_add(1);
        }
    }
    /// At most one job per call. Expired reservations are failed even if a host
    /// disappeared without completing them. Poll only on a background owner.
    pub fn begin(&mut self, now: u64) -> Option<Job> {
        let now = self.clock(now);
        let expired: Vec<_> = self
            .entries
            .iter()
            .filter_map(|(id, e)| {
                e.flight
                    .filter(|(_, deadline)| now >= *deadline)
                    .map(|_| *id)
            })
            .collect();
        for id in expired {
            self.failed(id, now);
        }
        let id = self
            .entries
            .iter()
            .filter(|(_, e)| e.flight.is_none() && e.due <= now)
            .min_by_key(|(id, e)| (e.due, **id))
            .map(|(id, _)| *id)?;
        let nonce = self.next().ok()?;
        let entry = self.entries.get_mut(&id)?;
        if entry.failures >= self.limits.attempts {
            entry.failures = 0;
        }
        let deadline = now.saturating_add(self.limits.timeout_ms);
        entry.flight = Some((nonce, deadline));
        self.metrics.attempts = self.metrics.attempts.saturating_add(1);
        Some(Job {
            id,
            nonce,
            deadline,
            selected: entry.selected.clone(),
            timeout_ms: self.limits.timeout_ms,
        })
    }
    pub fn complete(
        &mut self,
        job: Job,
        result: Result<Vec<u8>, Error>,
        now: u64,
    ) -> Result<(), Error> {
        let now = self.clock(now);
        let entry = self.entries.get(&job.id).ok_or(Error::Obsolete)?;
        if entry.flight != Some((job.nonce, job.deadline)) || job.selected != entry.selected {
            return Err(Error::Obsolete);
        }
        let staged = if now >= job.deadline {
            Err(Error::Timeout)
        } else {
            result
                .and_then(|bytes| Snapshot::parse(&bytes, &entry.selected))
                .and_then(|snapshot| {
                    if let Some(current) = &entry.snapshot {
                        if snapshot.revision() < current.revision() {
                            return Err(Error::Regression);
                        }
                        if snapshot.revision() == current.revision()
                            && snapshot.canonical != current.canonical
                        {
                            return Err(Error::Conflict);
                        }
                    }
                    // Retain scope accounting and charge parsed aggregate plus canonical bytes.
                    let previous = entry.snapshot.as_ref().map_or(0, |s| {
                        s.canonical.len() * 2 + std::mem::size_of::<Snapshot>()
                    });
                    let bytes = entry.bytes - previous
                        + snapshot.canonical.len() * 2
                        + std::mem::size_of::<Snapshot>();
                    if self.used() - entry.bytes + bytes > self.limits.bytes {
                        return Err(Error::Capacity);
                    }
                    Ok((snapshot, bytes))
                })
        };
        match staged {
            Ok((snapshot, bytes)) => {
                let e = self.entries.get_mut(&job.id).ok_or(Error::Obsolete)?;
                e.snapshot = Some(Arc::new(snapshot));
                e.bytes = bytes;
                e.fresh = Some(now);
                e.flight = None;
                e.failures = 0;
                e.due = now.saturating_add(self.limits.refresh_ms);
                self.metrics.installs = self.metrics.installs.saturating_add(1);
                Ok(())
            }
            Err(error) => {
                self.metrics.rejections = self.metrics.rejections.saturating_add(1);
                if error == Error::Conflict {
                    self.metrics.conflicts = self.metrics.conflicts.saturating_add(1);
                }
                self.failed(job.id, now);
                Err(error)
            }
        }
    }
}
