use super::{Error, Key, MAX_SOURCES, Observation, Scope};
use std::sync::{Arc, Mutex, MutexGuard, TryLockError};
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub scopes: usize,
    pub observations_per_scope: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            scopes: 16,
            observations_per_scope: 16,
        }
    }
}
#[derive(Debug, Clone)]
pub struct Snapshot {
    scope: Scope,
    observations: Arc<[Observation]>,
}
impl Snapshot {
    pub fn scope(&self) -> &Scope {
        &self.scope
    }
    pub fn observations(&self) -> &[Observation] {
        &self.observations
    }
    pub fn find(&self, key: &Key) -> Option<&Observation> {
        self.observations.iter().find(|o| &o.key == key)
    }
}
#[derive(Debug, Clone)]
struct Entry {
    scope: Scope,
    observations: Vec<Observation>,
}
/// Administrative publication and snapshot acquisition only. Never called by
/// the guard: contention is a returned error, not a reason to wait in-path.
pub struct Store {
    limits: Limits,
    entries: Mutex<Vec<Entry>>,
}
impl Store {
    pub fn new(limits: Limits) -> Result<Self, Error> {
        if limits.scopes == 0
            || limits.scopes > 128
            || limits.observations_per_scope == 0
            || limits.observations_per_scope > MAX_SOURCES
        {
            return Err(Error::Capacity);
        }
        Ok(Self {
            limits,
            entries: Mutex::new(Vec::new()),
        })
    }
    fn lock(&self) -> Result<MutexGuard<'_, Vec<Entry>>, Error> {
        self.entries.try_lock().map_err(|e| match e {
            TryLockError::WouldBlock => Error::Busy,
            TryLockError::Poisoned(_) => Error::Poisoned,
        })
    }
    pub fn record(&self, observations: &[Observation]) -> Result<(), Error> {
        self.record_inner(observations, false)
    }
    /// Background delivery may replay only the identical observation. Sequence
    /// or value conflicts retain the same stale fence as direct publication.
    pub fn record_replayed(&self, observations: &[Observation]) -> Result<(), Error> {
        self.record_inner(observations, true)
    }
    /// Revoke only the exact value owned by the changed recipe. Preserve its
    /// sequence fence so a delayed success cannot resurrect revoked evidence.
    pub fn invalidate_if_current(&self, expected: &Observation) -> Result<(), Error> {
        expected.scope.validate()?;
        expected.key.validate()?;
        let mut entries = self.lock()?;
        if let Some(entry) = entries
            .iter_mut()
            .find(|entry| entry.scope == expected.scope)
            && let Some(prior) = entry
                .observations
                .iter_mut()
                .find(|prior| **prior == *expected)
        {
            prior.value = None;
        }
        Ok(())
    }
    fn record_inner(&self, observations: &[Observation], replay: bool) -> Result<(), Error> {
        if observations.len() > MAX_SOURCES {
            return Err(Error::Capacity);
        }
        for (i, o) in observations.iter().enumerate() {
            o.scope.validate()?;
            o.key.validate()?;
            if o.sequence == 0
                || observations[..i]
                    .iter()
                    .any(|p| p.scope == o.scope && p.key == o.key)
            {
                return Err(Error::Invalid);
            }
        }
        let mut entries = self.lock()?;
        let mut staged = entries.clone();
        for o in observations {
            let position = staged.iter().position(|e| e.scope == o.scope);
            let entry = if let Some(i) = position {
                &mut staged[i]
            } else {
                if staged.len() >= self.limits.scopes {
                    return Err(Error::Capacity);
                }
                staged.push(Entry {
                    scope: o.scope.clone(),
                    observations: Vec::new(),
                });
                staged.last_mut().ok_or(Error::Invalid)?
            };
            if let Some(prior) = entry.observations.iter_mut().find(|p| p.key == o.key) {
                if replay && o == prior {
                    continue;
                }
                if o.sequence <= prior.sequence || o.at_ms < prior.at_ms {
                    return Err(Error::Stale);
                }
                *prior = o.clone();
            } else {
                if entry.observations.len() >= self.limits.observations_per_scope {
                    return Err(Error::Capacity);
                }
                entry.observations.push(o.clone());
            }
        }
        *entries = staged;
        Ok(())
    }
    pub fn snapshot(&self, scope: &Scope) -> Result<Snapshot, Error> {
        scope.validate()?;
        let entries = self.lock()?;
        let observations = entries
            .iter()
            .find(|e| &e.scope == scope)
            .map_or_else(Vec::new, |e| e.observations.clone());
        Ok(Snapshot {
            scope: scope.clone(),
            observations: observations.into(),
        })
    }
    /// Explicit host retention; snapshots already handed out remain valid.
    pub fn remove(&self, scope: &Scope) -> Result<(), Error> {
        scope.validate()?;
        self.lock()?.retain(|e| &e.scope != scope);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn contention_returns_busy_without_waiting_or_mutating()
    -> Result<(), Box<dyn std::error::Error>> {
        let store = Store::new(Limits::default())?;
        let scope = Scope {
            tenant: "t".into(),
            connection: "c".into(),
            vendor: "v".into(),
            workflow: "w".into(),
            entity: "e".into(),
            revision: "r".into(),
        };
        let held = store.entries.lock().map_err(|_| "lock")?;
        assert!(matches!(store.snapshot(&scope), Err(Error::Busy)));
        assert_eq!(store.record(&[]), Err(Error::Busy));
        drop(held);
        assert!(store.snapshot(&scope)?.observations().is_empty());
        Ok(())
    }
    #[test]
    fn poisoned_administrative_state_is_reported_without_panicking()
    -> Result<(), Box<dyn std::error::Error>> {
        let store = Arc::new(Store::new(Limits::default())?);
        let other = store.clone();
        let result = std::thread::spawn(move || {
            if let Ok(_held) = other.entries.lock() {
                panic!("injected publication panic");
            }
        })
        .join();
        assert!(result.is_err());
        assert_eq!(store.record(&[]), Err(Error::Poisoned));
        Ok(())
    }
}
