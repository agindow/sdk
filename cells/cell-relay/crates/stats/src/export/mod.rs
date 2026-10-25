//! TI-48: local staging is separate from background projection and delivery.
//! No sender, clock, profiler or filesystem is reachable from `Staging::close`.
#[cfg(not(target_arch = "wasm32"))]
pub mod native;
mod profile;
use crate::windows::{Error, Scope, Store, Window};
use chrono::{DateTime, Utc};
pub use profile::project;
use std::{collections::VecDeque, sync::Arc};

#[derive(Debug, Clone, Copy)]
pub struct StageLimits {
    pub windows: usize,
    pub bytes: usize,
}
/// Immutable detached observations; scalar values remain local to the process.
#[derive(Debug, Clone)]
pub struct Closed {
    scope: Scope,
    id: String,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    window: Arc<Window>,
    bytes: usize,
}
/// Caller-owned bounded handoff. Reject-new leaves source observations intact.
/// Transfer to a background owner outside guard evaluation; do not block on a lock.
pub struct Staging {
    limits: StageLimits,
    pending: VecDeque<Closed>,
    bytes: usize,
    rejected: u64,
}
impl Staging {
    pub fn new(limits: StageLimits) -> Result<Self, Error> {
        if limits.windows == 0 || limits.windows > 4096 || limits.bytes == 0 {
            return Err(Error::InvalidInput);
        }
        Ok(Self {
            limits,
            pending: VecDeque::new(),
            bytes: 0,
            rejected: 0,
        })
    }
    pub fn len(&self) -> usize {
        self.pending.len()
    }
    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }
    pub fn bytes(&self) -> usize {
        self.bytes
    }
    pub fn rejected(&self) -> u64 {
        self.rejected
    }
    pub fn close(
        &mut self,
        store: &mut Store,
        scope: &Scope,
        id: &str,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<(), Error> {
        let result = self.close_inner(store, scope, id, start, end);
        if result.is_err() {
            self.rejected = self.rejected.saturating_add(1);
        }
        result
    }
    fn close_inner(
        &mut self,
        store: &mut Store,
        scope: &Scope,
        id: &str,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<(), Error> {
        if id.is_empty() || id.len() > 256 {
            return Err(Error::InvalidInput);
        }
        let bytes = store
            .export_bytes(scope)?
            .checked_add(id.len() + std::mem::size_of::<Closed>())
            .ok_or(Error::ResourceLimit)?;
        if self.pending.len() >= self.limits.windows
            || bytes > self.limits.bytes.saturating_sub(self.bytes)
        {
            return Err(Error::ResourceLimit);
        }
        let window = store.close_export(scope, start, end)?;
        self.pending.push_back(Closed {
            scope: scope.clone(),
            id: id.into(),
            start,
            end,
            window,
            bytes,
        });
        self.bytes += bytes;
        Ok(())
    }
    /// Borrow until background durable staging succeeds, then acknowledge with pop.
    pub fn front(&self) -> Option<&Closed> {
        self.pending.front()
    }
    pub fn pop(&mut self) -> Option<Closed> {
        let closed = self.pending.pop_front()?;
        self.bytes -= closed.bytes;
        Some(closed)
    }
}
