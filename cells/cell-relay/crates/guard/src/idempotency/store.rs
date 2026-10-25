use super::{
    Access, Bindings, DurabilityStatus, Forwarding, Limits, Rejection, Replay, Request, scope,
};
use agw_policy::Operation;
use std::{
    collections::HashMap,
    sync::{
        Arc, Condvar, Mutex, MutexGuard, PoisonError,
        atomic::{AtomicUsize, Ordering},
    },
    thread::{self, ThreadId},
    time::{Duration, Instant},
};

type Key = (String, String);
struct Entry<T> {
    state: Mutex<State<T>>,
    changed: Condvar,
}
enum State<T> {
    Running(ThreadId),
    Ready {
        value: Arc<T>,
        durability: DurabilityStatus,
        expires: Option<Instant>,
    },
    Indeterminate {
        expires: Option<Instant>,
    },
}
impl<T> Entry<T> {
    fn expired(&self, now: Instant) -> bool {
        match &*lock(&self.state) {
            State::Running(_) => false,
            State::Ready { expires, .. } | State::Indeterminate { expires } => {
                expires.is_some_and(|at| now >= at)
            }
        }
    }
}
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    // No caller callbacks/destructors run under these locks. Recovering poison
    // retains every reservation rather than silently starting another write.
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
struct Reservation<'a, T> {
    entry: &'a Entry<T>,
    window: Duration,
    finished: bool,
}
impl<T> Drop for Reservation<'_, T> {
    fn drop(&mut self) {
        if !self.finished {
            *lock(&self.entry.state) = State::Indeterminate {
                expires: Instant::now().checked_add(self.window),
            };
            self.entry.changed.notify_all();
        }
    }
}
struct Waiter<'a>(&'a AtomicUsize);
impl Drop for Waiter<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
}

/// Process-local replay domain for one authenticated tenant. Clones of the
/// returned Arc preserve original outcome identity. Hosts bound retained values.
pub struct Store<T> {
    tenant: String,
    limits: Limits,
    bindings: Bindings,
    entries: Mutex<HashMap<Key, Arc<Entry<T>>>>,
    waiting: AtomicUsize,
}
impl<T> Store<T> {
    pub fn new(tenant: &str, limits: Limits, bindings: Bindings) -> Result<Self, Rejection> {
        if !scope(tenant)
            || limits.entries == 0
            || limits.entries > 10_000
            || limits.window.is_zero()
            || limits.window > Duration::from_secs(7 * 86400)
        {
            return Err(Rejection::invalid("invalid tenant or replay limits"));
        }
        Ok(Self {
            tenant: tenant.into(),
            limits,
            bindings,
            entries: Mutex::new(HashMap::new()),
            waiting: AtomicUsize::new(0),
        })
    }
    pub fn tenant(&self) -> &str {
        &self.tenant
    }
    /// Observability only; never use this racy count to authorize a dispatch.
    pub fn waiting(&self) -> usize {
        self.waiting.load(Ordering::Relaxed)
    }

    pub fn execute(
        &self,
        request: Request<'_>,
        operation: impl FnOnce(&Forwarding<'_>) -> T,
    ) -> Result<Access<T>, Rejection> {
        request.validate()?;
        let forwarding = self.bindings.prepare(request);
        if request.operation == Operation::Read {
            return Ok(Access::Completed(Replay {
                value: Arc::new(operation(&forwarding)),
                replayed: false,
                durability: forwarding.durability_status,
            }));
        }
        let key = request
            .key
            .ok_or_else(|| Rejection::invalid("missing idempotency_key"))?;
        let key = (request.connection.to_owned(), key.to_owned());
        let (entry, leader) = self.admit(key)?;
        if !leader {
            return Ok(self.follow(&entry));
        }
        let mut reservation = Reservation {
            entry: &entry,
            window: self.limits.window,
            finished: false,
        };
        // Caller execution and unwind are outside all recovery/lock boundaries.
        let value = Arc::new(operation(&forwarding));
        *lock(&entry.state) = State::Ready {
            value: Arc::clone(&value),
            durability: forwarding.durability_status,
            expires: Instant::now().checked_add(self.limits.window),
        };
        reservation.finished = true;
        entry.changed.notify_all();
        Ok(Access::Completed(Replay {
            value,
            replayed: false,
            durability: forwarding.durability_status,
        }))
    }

    fn admit(&self, key: Key) -> Result<(Arc<Entry<T>>, bool), Rejection> {
        let mut map = lock(&self.entries);
        let now = Instant::now();
        let expired: Vec<_> = map
            .iter()
            .filter(|(_, entry)| entry.expired(now))
            .map(|(key, _)| key.clone())
            .collect();
        let retired: Vec<_> = expired.iter().filter_map(|key| map.remove(key)).collect();
        let admission = if let Some(entry) = map.get(&key) {
            Ok((Arc::clone(entry), false))
        } else if map.len() >= self.limits.entries {
            Err(Rejection::full())
        } else {
            let entry = Arc::new(Entry {
                state: Mutex::new(State::Running(thread::current().id())),
                changed: Condvar::new(),
            });
            map.insert(key, Arc::clone(&entry));
            Ok((entry, true))
        };
        drop(map);
        // Expired caller-owned outcomes can have arbitrary destructors. They
        // cannot poison the map or turn cleanup into a second execution.
        let _ = crate::availability::contain(std::panic::AssertUnwindSafe(|| {
            drop(retired);
            Ok(())
        }));
        admission
    }

    fn follow(&self, entry: &Entry<T>) -> Access<T> {
        let mut state = lock(&entry.state);
        let mut waiter = None;
        loop {
            match &*state {
                State::Ready {
                    value, durability, ..
                } => {
                    return Access::Completed(Replay {
                        value: Arc::clone(value),
                        replayed: true,
                        durability: *durability,
                    });
                }
                State::Indeterminate { .. } => return Access::Indeterminate,
                State::Running(owner) if *owner == thread::current().id() => {
                    return Access::InProgress;
                }
                State::Running(_) => {
                    if waiter.is_none() {
                        self.waiting.fetch_add(1, Ordering::Relaxed);
                        waiter = Some(Waiter(&self.waiting));
                    }
                    state = entry
                        .changed
                        .wait(state)
                        .unwrap_or_else(PoisonError::into_inner);
                }
            }
        }
    }
}
