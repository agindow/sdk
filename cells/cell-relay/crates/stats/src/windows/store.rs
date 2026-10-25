use super::{Availability, Error, Limits, Sample, Scope, Selection, Snapshot, Window, extract};
use agw_inspector::Inspection;
use chrono::{DateTime, Utc};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

struct Entry {
    baseline: Option<Arc<Window>>,
    live: Arc<Window>,
    available: bool,
}
impl Entry {
    fn bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.live.bytes()
            + self.baseline.as_ref().map_or(0, |w| w.bytes())
    }
}
/// Local only. All mutations require exclusive access; snapshots are immutable.
/// Accounted bytes exclude allocator slack and externally retained snapshots.
pub struct Store {
    limits: Limits,
    entries: BTreeMap<Scope, Entry>,
    bytes: usize,
}
impl Store {
    pub fn new(limits: Limits) -> Result<Self, Error> {
        if limits.minimum == 0
            || limits.minimum > limits.samples
            || limits.samples > 4096
            || limits.scopes == 0
            || limits.bytes == 0
            || limits.batch == 0
            || limits.batch > limits.scopes
            || limits.inspection_nodes == 0
            || limits.inspection_bytes == 0
        {
            return Err(Error::InvalidInput);
        }
        Ok(Self {
            limits,
            entries: BTreeMap::new(),
            bytes: 0,
        })
    }
    pub fn bytes(&self) -> usize {
        self.bytes
    }
    pub fn scopes(&self) -> usize {
        self.entries.len()
    }

    /// A batch represents one call/side. Validation and quota preflight precede
    /// every retained clone, and every error leaves all entries unchanged.
    pub fn record(
        &mut self,
        selected: &[Selection],
        inspection: &Inspection,
        at: DateTime<Utc>,
    ) -> Result<(), Error> {
        if selected.is_empty() || selected.len() > self.limits.batch {
            return Err(Error::ResourceLimit);
        }
        let root = extract::complete(inspection, self.limits)?;
        let values = selected
            .iter()
            .map(|selection| {
                extract::scalar(root, &selection.scope.field).map(|value| (selection, value))
            })
            .collect::<Result<Vec<_>, _>>()?;
        self.record_values(&values, at)
    }
    /// Shared atomic admission after bounded, complete extraction. No caller can
    /// reach this without validating the entire inspection first.
    pub(super) fn record_values(
        &mut self,
        selected: &[(&Selection, Option<&agw_inspector::Value>)],
        at: DateTime<Utc>,
    ) -> Result<(), Error> {
        if selected.is_empty() || selected.len() > self.limits.batch {
            return Err(Error::ResourceLimit);
        }
        let mut seen = BTreeSet::new();
        let mut staged = Vec::new();
        let mut bytes = self.bytes;
        let mut scopes = self.entries.len();
        for &(selection, value) in selected {
            let key_bytes = selection.scope.bytes()?;
            if !seen.insert(&selection.scope) {
                return Err(Error::InvalidInput);
            }
            if let Some(entry) = self.entries.get(&selection.scope) {
                if !entry.available {
                    return Err(Error::ProviderUnavailable);
                }
                if entry.live.projection != selection.projection {
                    return Err(Error::InvalidInput);
                }
                if entry.live.samples.len() >= self.limits.samples {
                    return Err(Error::ResourceLimit);
                }
            } else {
                scopes = scopes.checked_add(1).ok_or(Error::ResourceLimit)?;
                bytes = bytes
                    .checked_add(key_bytes)
                    .and_then(|b| {
                        b.checked_add(std::mem::size_of::<Entry>() + std::mem::size_of::<Window>())
                    })
                    .ok_or(Error::ResourceLimit)?;
            }
            bytes = bytes
                .checked_add(Sample::bytes(value))
                .ok_or(Error::ResourceLimit)?;
            if bytes > self.limits.bytes || scopes > self.limits.scopes {
                return Err(Error::ResourceLimit);
            }
            // Existing detector limits apply to materialization as well. Bound
            // each retained scalar even when inspection's overall cap is larger.
            if matches!(value, Some(agw_inspector::Value::String(s)) if s.len() > 4096)
                || matches!(value, Some(agw_inspector::Value::Number(n)) if n.lexeme_len() > 4096)
            {
                return Err(Error::ResourceLimit);
            }
            staged.push((selection, value));
        }
        for (selection, value) in staged {
            let entry = self
                .entries
                .entry(selection.scope.clone())
                .or_insert_with(|| Entry {
                    baseline: None,
                    live: Arc::new(Window {
                        samples: Vec::new(),
                        projection: selection.projection,
                    }),
                    available: true,
                });
            Arc::make_mut(&mut entry.live).samples.push(Sample {
                value: value.cloned(),
                at,
            });
        }
        self.bytes = bytes;
        Ok(())
    }
    /// Publish once per explicit scope generation, never implicitly from drift.
    pub fn publish(&mut self, scope: &Scope) -> Result<(), Error> {
        let entry = self.entries.get_mut(scope).ok_or(Error::UnknownScope)?;
        if !entry.available {
            return Err(Error::ProviderUnavailable);
        }
        if entry.baseline.is_some() {
            return Err(Error::AlreadyPublished);
        }
        if entry.live.samples.len() < self.limits.minimum {
            return Err(Error::InsufficientSamples);
        }
        let bytes = self
            .bytes
            .checked_add(std::mem::size_of::<Window>())
            .ok_or(Error::ResourceLimit)?;
        if bytes > self.limits.bytes {
            return Err(Error::ResourceLimit);
        }
        let empty = Arc::new(Window {
            samples: Vec::new(),
            projection: entry.live.projection,
        });
        entry.baseline = Some(std::mem::replace(&mut entry.live, empty));
        self.bytes = bytes;
        Ok(())
    }
    /// TI-48: detach the complete live window without publishing a baseline.
    /// The staging owner preflights capacity before calling this local method.
    pub(crate) fn close_export(
        &mut self,
        scope: &Scope,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Arc<Window>, Error> {
        let entry = self.entries.get_mut(scope).ok_or(Error::UnknownScope)?;
        if !entry.available
            || entry.live.samples.is_empty()
            || start >= end
            || entry
                .live
                .samples
                .iter()
                .any(|s| s.at < start || s.at >= end)
            || !start.timestamp_subsec_nanos().is_multiple_of(1_000_000)
            || !end.timestamp_subsec_nanos().is_multiple_of(1_000_000)
        {
            return Err(Error::InvalidInput);
        }
        self.bytes -= entry.live.bytes() - std::mem::size_of::<Window>();
        let empty = Arc::new(Window {
            samples: Vec::new(),
            projection: entry.live.projection,
        });
        Ok(std::mem::replace(&mut entry.live, empty))
    }
    pub(crate) fn export_bytes(&self, scope: &Scope) -> Result<usize, Error> {
        let entry = self.entries.get(scope).ok_or(Error::UnknownScope)?;
        Ok(scope.bytes()? + entry.live.bytes())
    }
    /// Explicit live-window boundary. The frozen reference is retained.
    pub fn advance(&mut self, scope: &Scope) -> Result<(), Error> {
        let entry = self.entries.get_mut(scope).ok_or(Error::UnknownScope)?;
        if entry.baseline.is_none() {
            return Err(Error::InsufficientSamples);
        }
        self.bytes -= entry.live.bytes() - std::mem::size_of::<Window>();
        entry.live = Arc::new(Window {
            samples: Vec::new(),
            projection: entry.live.projection,
        });
        Ok(())
    }
    /// Outer refresh errors are local state; this never calls a provider.
    pub fn set_available(&mut self, scope: &Scope, available: bool) -> Result<(), Error> {
        self.entries
            .get_mut(scope)
            .ok_or(Error::UnknownScope)?
            .available = available;
        Ok(())
    }
    pub fn snapshot(&self, scope: &Scope) -> Availability {
        // Bound comparisons/cloning even for untrusted lookup keys.
        if let Err(error) = scope.bytes() {
            return Availability::Unavailable(error);
        }
        let Some(entry) = self.entries.get(scope) else {
            return Availability::Unavailable(Error::UnknownScope);
        };
        if !entry.available {
            return Availability::Unavailable(Error::ProviderUnavailable);
        }
        let baseline = entry.baseline.as_ref().map_or(0, |w| w.samples.len());
        let live = entry.live.samples.len();
        if baseline < self.limits.minimum || live < self.limits.minimum {
            return Availability::Warming { baseline, live };
        }
        match &entry.baseline {
            Some(baseline) => Availability::Ready(Snapshot {
                scope: Arc::new(scope.clone()),
                baseline: Arc::clone(baseline),
                live: Arc::clone(&entry.live),
                accounted_bytes: baseline.bytes() + entry.live.bytes(),
            }),
            None => Availability::Unavailable(Error::UnknownScope),
        }
    }
    /// Explicit lifecycle removal; capacity pressure never invokes it.
    pub fn remove(&mut self, scope: &Scope) -> bool {
        if let Some(entry) = self.entries.remove(scope) {
            // Already admitted keys have valid byte accounting.
            self.bytes -= entry.bytes()
                + std::mem::size_of::<Scope>()
                + scope.parts().iter().map(|s| s.len()).sum::<usize>();
            true
        } else {
            false
        }
    }
}
