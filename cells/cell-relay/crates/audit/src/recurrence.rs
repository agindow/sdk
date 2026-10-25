//! TI-29 recurrence: the flywheel's per-customer first_seen/occurrences.
//!
//! One signature is `(class, side, vendor, sorted changes)`. The map is local,
//! tenant-bound and bounded; full capacity refuses without evicting history or
//! mutating the caller's drift. Callers who need cross-instance or global
//! recurrence merge these local counts themselves — this is not the corpus.

use crate::Error;
use crate::contracts::audit_artifact::{Drift, DriftClass, DriftSide};
use chrono::{DateTime, Utc};
use std::collections::BTreeMap;
use std::num::NonZeroU64;

/// Default bounded signature count.
pub const DEFAULT_CAPACITY: usize = 4096;
/// Hard ceiling for a local signature map.
pub const MAX_CAPACITY: usize = 1_000_000;

type ChangeTuple = (String, String, Option<String>, Option<String>);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Signature {
    class: u8,
    side: u8,
    vendor: Option<String>,
    changes: Vec<ChangeTuple>,
}

impl Signature {
    fn of(drift: &Drift) -> Self {
        let mut changes: Vec<ChangeTuple> = drift
            .changes
            .iter()
            .map(|change| {
                (
                    crate::text(&change.path),
                    crate::text(&change.kind),
                    change.from.clone(),
                    change.to.clone(),
                )
            })
            .collect();
        // Identity is the set of changes, not the order a detector emitted.
        changes.sort();
        Self {
            class: match drift.class {
                DriftClass::Shape => 0,
                DriftClass::Semantic => 1,
                DriftClass::Unknown => 2,
            },
            side: match drift.side {
                DriftSide::Request => 0,
                DriftSide::Response => 1,
                DriftSide::Both => 2,
            },
            vendor: drift.vendor.as_ref().map(crate::text),
            changes,
        }
    }
}

/// Bounded local recurrence tracker. One instance is one `(tenant, process)`
/// view; the caller owns any merge across instances.
#[derive(Debug, Clone)]
pub struct Recurrence {
    capacity: usize,
    entries: BTreeMap<Signature, (DateTime<Utc>, u64)>,
}

/// One staged signature update. Dropping it leaves the tracker unchanged;
/// exclusive ownership prevents another observation from invalidating it.
pub(crate) struct Pending<'a> {
    tracker: &'a mut Recurrence,
    signature: Signature,
    first_seen: DateTime<Utc>,
    count: u64,
}

impl Pending<'_> {
    pub(crate) fn commit(self) {
        self.tracker
            .entries
            .insert(self.signature, (self.first_seen, self.count));
    }
}

impl Recurrence {
    pub fn new(capacity: usize) -> Result<Self, Error> {
        if capacity == 0 || capacity > MAX_CAPACITY {
            return Err(Error::Invalid("recurrence capacity must be 1..=1000000"));
        }
        Ok(Self {
            capacity,
            entries: BTreeMap::new(),
        })
    }

    /// Stamp `first_seen`/`occurrences` on the drift, or refuse at capacity.
    ///
    /// Refusal leaves `drift` untouched: publication falls back to an unstamped
    /// artifact instead of dropping it.
    pub fn record(&mut self, drift: &mut Drift, ts: DateTime<Utc>) -> Result<(), Error> {
        self.prepare(drift, ts)?.commit();
        Ok(())
    }

    /// Stamp the candidate artifact without consuming capacity or counting it
    /// until the publisher has admitted that exact artifact into storage.
    pub(crate) fn prepare(
        &mut self,
        drift: &mut Drift,
        ts: DateTime<Utc>,
    ) -> Result<Pending<'_>, Error> {
        let signature = Signature::of(drift);
        let (first_seen, count) = if let Some((first_seen, count)) = self.entries.get(&signature) {
            (*first_seen, count.saturating_add(1))
        } else {
            if self.entries.len() >= self.capacity {
                return Err(Error::Capacity);
            }
            (ts, 1)
        };
        drift.first_seen = Some(first_seen);
        drift.occurrences = NonZeroU64::new(count);
        Ok(Pending {
            tracker: self,
            signature,
            first_seen,
            count,
        })
    }

    /// Number of tracked signatures.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether nothing has been tracked yet.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The configured signature ceiling.
    pub fn capacity(&self) -> usize {
        self.capacity
    }
}
