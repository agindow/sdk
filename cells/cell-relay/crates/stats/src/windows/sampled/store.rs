use super::{Availability, Draw, Error, Selection, Snapshot};
use crate::windows::{self, Limits, extract};
use agw_inspector::Inspection;
use chrono::{DateTime, Utc};
use std::sync::Arc;

/// Separate opt-in store. No export or guard path can discard sampling metadata.
pub struct Store {
    inner: windows::Store,
    limits: Limits,
}
impl Store {
    pub fn new(limits: Limits) -> Result<Self, windows::Error> {
        Ok(Self {
            inner: windows::Store::new(limits)?,
            limits,
        })
    }
    pub fn bytes(&self) -> usize {
        self.inner.bytes()
    }
    pub fn scopes(&self) -> usize {
        self.inner.scopes()
    }
    /// One batch is one complete call/side. Rejection admits nothing, including
    /// when an unavailable array accompanies otherwise valid selections.
    pub fn record(
        &mut self,
        draws: &[Draw<'_>],
        inspection: &Inspection,
        at: DateTime<Utc>,
    ) -> Result<(), Error> {
        if draws.is_empty() || draws.len() > self.limits.batch {
            return Err(windows::Error::ResourceLimit.into());
        }
        let (root, mut work) = extract::complete_budget(inspection, self.limits)?;
        let keys = draws
            .iter()
            .map(|d| d.selection.key())
            .collect::<Result<Vec<_>, _>>()?;
        let values = draws
            .iter()
            .zip(&keys)
            .map(|(draw, key)| {
                draw.selection
                    .sample(root, draw.entropy, &mut work)
                    .map(|value| (key, value))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(self.inner.record_values(&values, at)?)
    }
    pub fn publish(&mut self, selected: &Selection) -> Result<(), Error> {
        Ok(self.inner.publish(&selected.key()?.scope)?)
    }
    pub fn advance(&mut self, selected: &Selection) -> Result<(), Error> {
        Ok(self.inner.advance(&selected.key()?.scope)?)
    }
    pub fn remove(&mut self, selected: &Selection) -> Result<bool, Error> {
        Ok(self.inner.remove(&selected.key()?.scope))
    }
    pub fn set_available(&mut self, selected: &Selection, available: bool) -> Result<(), Error> {
        Ok(self
            .inner
            .set_available(&selected.key()?.scope, available)?)
    }
    pub fn snapshot(&self, selected: &Selection) -> Availability {
        let key = match selected.key() {
            Ok(key) => key,
            Err(e) => return Availability::Unavailable(e),
        };
        match self.inner.snapshot(&key.scope) {
            windows::Availability::Unavailable(e) => Availability::Unavailable(e.into()),
            windows::Availability::Warming { baseline, live } => {
                Availability::Warming { baseline, live }
            }
            windows::Availability::Ready(inner) => {
                if inner.live().projection != selected.projection {
                    return Availability::Unavailable(windows::Error::InvalidInput.into());
                }
                Availability::Ready(Snapshot {
                    selection: Arc::new(selected.clone()),
                    inner,
                })
            }
        }
    }
}
