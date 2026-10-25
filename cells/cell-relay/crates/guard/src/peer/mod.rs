//! TII-27 host-owned background publication into TI-23. No guard callback,
//! network request, rule activation or customer-pin mutation exists here.
mod wire;
use agw_invariants::cross_endpoint::{Key, Observation, Scope, Store};
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex, MutexGuard, TryLockError};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Invalid,
    Scope,
    Expired,
    Conflict,
    Capacity,
    Busy,
    Poisoned,
    Store(agw_invariants::cross_endpoint::Error),
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recipe {
    pub id: String,
    pub revision: u32,
    pub enabled: bool,
    pub source_url: String,
    pub scope: Scope,
    pub key: Key,
    pub expires_ms: i64,
}
#[derive(Debug, Clone)]
pub struct Receipt {
    pub id: String,
    pub recipe_revision: u32,
    pub attempt: u8,
    pub source: String,
    pub workflow: String,
    pub sequence: u64,
    pub available: bool,
    pub replayed: bool,
}
struct Config {
    recipe: Recipe,
    last: Option<Observation>,
}
struct Admission {
    job: String,
    recipe: String,
    revision: u32,
    sequence: u64,
    expires: i64,
    receipt: Option<(Vec<u8>, Receipt)>,
}
struct State {
    recipes: Vec<Config>,
    jobs: Vec<Admission>,
}
pub struct PeerPublisher {
    store: Arc<Store>,
    capacity: usize,
    state: Mutex<State>,
}
impl PeerPublisher {
    pub fn new(store: Arc<Store>, capacity: usize) -> Result<Self, Error> {
        if capacity == 0 || capacity > 1024 {
            return Err(Error::Capacity);
        }
        Ok(Self {
            store,
            capacity,
            state: Mutex::new(State {
                recipes: Vec::new(),
                jobs: Vec::new(),
            }),
        })
    }
    fn lock(&self) -> Result<MutexGuard<'_, State>, Error> {
        self.state.try_lock().map_err(|error| match error {
            TryLockError::WouldBlock => Error::Busy,
            TryLockError::Poisoned(_) => Error::Poisoned,
        })
    }
    /// The host authenticates configuration and owns restoring it with any restored
    /// TI-23 state. The default store/publisher are explicitly process-local.
    pub fn configure(&self, recipe: Recipe) -> Result<(), Error> {
        recipe.scope.validate().map_err(Error::Store)?;
        recipe.key.validate().map_err(Error::Store)?;
        wire::authority(&recipe.source_url)?;
        wire::label(&recipe.id)?;
        if recipe.revision == 0
            || recipe.key.endpoint.side != agw_invariants::cross_endpoint::Side::Response
        {
            return Err(Error::Invalid);
        }
        let mut state = self.lock()?;
        if let Some(prior) = state
            .recipes
            .iter_mut()
            .find(|entry| entry.recipe.id == recipe.id)
        {
            if prior.recipe.revision >= recipe.revision {
                return if prior.recipe == recipe {
                    Ok(())
                } else {
                    Err(Error::Conflict)
                };
            }
            if let Some(last) = &prior.last {
                self.store
                    .invalidate_if_current(last)
                    .map_err(Error::Store)?;
            }
            *prior = Config { recipe, last: None };
        } else {
            if state.recipes.len() >= 64 {
                return Err(Error::Capacity);
            }
            state.recipes.push(Config { recipe, last: None });
        }
        Ok(())
    }
    /// Explicit pre-acquisition host admission; a publication cannot invent a job,
    /// workflow, source, quantity or causal sequence on arrival.
    pub fn admit(
        &self,
        job: &str,
        recipe: &str,
        sequence: u64,
        expires: i64,
        now: i64,
    ) -> Result<(), Error> {
        wire::label(job)?;
        if sequence == 0 || expires <= now || i128::from(expires) - i128::from(now) > 600000 {
            return Err(Error::Invalid);
        }
        let mut state = self.lock()?;
        let configured = state
            .recipes
            .iter()
            .find(|entry| entry.recipe.id == recipe)
            .ok_or(Error::Scope)?;
        if !configured.recipe.enabled || configured.recipe.expires_ms < expires {
            return Err(Error::Expired);
        }
        let revision = configured.recipe.revision;
        if let Some(prior) = state
            .jobs
            .iter()
            .find(|entry| entry.job == job && entry.recipe == recipe)
        {
            return if prior.revision == revision
                && prior.sequence == sequence
                && prior.expires == expires
            {
                Ok(())
            } else {
                Err(Error::Conflict)
            };
        }
        state.jobs.retain(|entry| entry.expires > now);
        if state.jobs.len() >= self.capacity {
            return Err(Error::Capacity);
        }
        state.jobs.push(Admission {
            job: job.into(),
            recipe: recipe.into(),
            revision,
            sequence,
            expires,
            receipt: None,
        });
        Ok(())
    }
    pub fn publish(&self, bytes: &[u8], now: i64) -> Result<Receipt, Error> {
        let parsed = wire::parse(bytes, now)?;
        let mut state = self.lock()?;
        let recipe_index = state
            .recipes
            .iter()
            .position(|entry| entry.recipe.id == parsed.recipe_id)
            .ok_or(Error::Scope)?;
        let recipe = &state.recipes[recipe_index].recipe;
        if !recipe.enabled || recipe.revision != parsed.revision || recipe.expires_ms <= now {
            return Err(Error::Expired);
        }
        if recipe.scope != parsed.observation.scope
            || recipe.key != parsed.observation.key
            || recipe.source_url != parsed.source
        {
            return Err(Error::Scope);
        }
        let job_index = state
            .jobs
            .iter()
            .position(|entry| entry.job == parsed.job && entry.recipe == parsed.recipe_id)
            .ok_or(Error::Scope)?;
        let job = &state.jobs[job_index];
        if job.revision != parsed.revision
            || job.sequence != parsed.observation.sequence
            || job.expires != parsed.expires
            || job.expires <= now
        {
            return Err(Error::Scope);
        }
        if let Some((hash, receipt)) = &job.receipt {
            if hash != &parsed.hash {
                return Err(Error::Conflict);
            }
            let mut replay = receipt.clone();
            replay.replayed = true;
            return Ok(replay);
        }
        self.store
            .record_replayed(std::slice::from_ref(&parsed.observation))
            .map_err(Error::Store)?;
        let receipt = Receipt {
            id: parsed.id,
            recipe_revision: parsed.revision,
            attempt: parsed.attempt,
            source: parsed.source,
            workflow: parsed.observation.scope.workflow.clone(),
            sequence: parsed.observation.sequence,
            available: parsed.observation.value.is_some(),
            replayed: false,
        };
        state.recipes[recipe_index].last = Some(parsed.observation);
        state.jobs[job_index].receipt = Some((parsed.hash, receipt.clone()));
        Ok(receipt)
    }
}
pub fn publication_identity(
    tenant: &str,
    job: &str,
    recipe: &str,
    revision: u32,
    sequence: u64,
) -> Result<String, Error> {
    let bytes = serde_json::to_vec(&(tenant, job, recipe, revision, sequence.to_string()))
        .map_err(|_| Error::Invalid)?;
    Ok(format!("peer_{:x}", Sha256::digest(bytes)))
}
