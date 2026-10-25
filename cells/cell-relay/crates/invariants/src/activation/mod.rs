//! TI-54 background-only qualification. No controller method belongs in a call path.
mod qualify;
mod types;
use crate::{
    MAX_RULES, compile,
    store::{self, Controls, Limits, Store},
    valid_text,
};
use std::sync::Arc;
pub use types::*;

pub struct Prepared {
    origin: Arc<()>,
    revision: u64,
    index: usize,
    decision: Decision,
    generation: Option<Arc<Generation>>,
}
impl Prepared {
    pub fn decision(&self) -> &Decision {
        &self.decision
    }
}
/// Exclusive administrative ownership is the publication serialization boundary.
/// Readers retain only `Arc<Generation>`, never a controller lock or dependency.
pub struct Controller {
    origin: Arc<()>,
    revision: u64,
    capacity: usize,
    scope: Scope,
    policy: Option<Policy>,
    policy_floor: u64,
    controls: Controls,
    receipts: Vec<Receipt>,
    fault: Option<(Reason, Receipt)>,
    candidates: Vec<Candidate>,
    decisions: Vec<Decision>,
    active: Option<Arc<Generation>>,
}
impl Controller {
    pub fn new(scope: Scope, capacity: usize) -> Result<Self, Error> {
        if !qualify::scope_valid(&scope) || capacity == 0 || capacity > 1024 {
            return Err(Error::Invalid);
        }
        Ok(Self {
            origin: Arc::new(()),
            revision: 0,
            capacity,
            scope,
            policy: None,
            policy_floor: 0,
            controls: Controls::default(),
            receipts: vec![],
            fault: None,
            candidates: vec![],
            decisions: vec![],
            active: None,
        })
    }
    pub fn active(&self) -> Option<Arc<Generation>> {
        self.active.clone()
    }
    pub fn decisions(&self) -> &[Decision] {
        &self.decisions
    }
    pub fn configure(&mut self, policy: Option<Policy>, controls: Controls) -> Result<(), Error> {
        if policy.as_ref().is_some_and(|p| {
            p.scope != self.scope || p.revision < self.policy_floor || !qualify::policy_valid(p)
        }) || controls.disabled.len() > MAX_RULES
            || controls.overrides.len() > MAX_RULES
            || controls
                .overrides
                .iter()
                .any(|r| !qualify::definition_valid(r))
            || controls.pin == Some(0)
            || controls
                .disabled
                .iter()
                .enumerate()
                .any(|(i, id)| !valid_text(id) || controls.disabled[..i].contains(id))
        {
            return Err(Error::Invalid);
        }
        if let (Some(old), Some(new)) = (&self.policy, &policy)
            && (new.revision < old.revision || (new.revision == old.revision && new != old))
        {
            return Err(Error::Conflict);
        }
        // Validate customer replacements against the current bindings before editing.
        if let Some(g) = &self.active {
            build(&g.candidate, &controls)?;
        }
        let revision = self.revision.checked_add(1).ok_or(Error::Capacity)?;
        let floor = if let Some(p) = &policy {
            p.revision
        } else if let Some(p) = &self.policy {
            p.revision.checked_add(1).ok_or(Error::Capacity)?
        } else {
            self.policy_floor
        };
        self.revision = revision;
        self.policy_floor = floor;
        self.policy = policy;
        self.controls = controls;
        Ok(())
    }
    pub fn record(&mut self, receipt: Receipt) -> Result<(), Error> {
        if receipt.scope != self.scope
            || receipt.id == 0
            || receipt.candidate == 0
            || receipt.policy == 0
            || !valid_text(&receipt.training)
            || !valid_text(&receipt.holdout)
        {
            return Err(Error::Invalid);
        }
        if let Some(old) = self.receipts.iter().find(|r| r.id == receipt.id) {
            if old == &receipt {
                return Ok(());
            }
            self.fault.get_or_insert((Reason::Contradictory, receipt));
            return Err(Error::Conflict);
        }
        if self.receipts.len() >= self.capacity {
            self.fault.get_or_insert((Reason::Incomplete, receipt));
            return Err(Error::Capacity);
        }
        self.revision = self.revision.checked_add(1).ok_or(Error::Capacity)?;
        self.receipts.push(receipt);
        Ok(())
    }
    pub fn prepare(
        &mut self,
        candidate: Candidate,
        stage: Stage,
        now: u64,
    ) -> Result<Prepared, Error> {
        if self.decisions.len() >= self.capacity {
            return Err(Error::Capacity);
        }
        if candidate.scope != self.scope
            || candidate.version == 0
            || (candidate.rules.is_empty() && candidate.peers.is_empty())
        {
            return Err(Error::Invalid);
        }
        if candidate.peers.len() > crate::cross_endpoint::MAX_RULES
            || candidate.rules.len() > crate::MAX_RULES
            || candidate.bindings.len() > crate::MAX_BINDINGS
        {
            return Err(Error::Capacity);
        }
        for (i, rule) in candidate.peers.iter().enumerate() {
            if rule.validate().is_err()
                || (rule.left.operation != self.scope.operation
                    && rule.right.operation != self.scope.operation)
                || candidate.peers[..i].iter().any(|p| p.id == rule.id)
                || candidate.rules.iter().any(|r| r.id == rule.id)
            {
                return Err(Error::Invalid);
            }
        }
        let bindings: Vec<_> = candidate.bindings.iter().map(String::as_str).collect();
        compile(&candidate.rules, &bindings).map_err(|_| Error::Compilation)?;
        if let Some(old) = self
            .candidates
            .iter()
            .find(|c| c.version == candidate.version)
        {
            if old != &candidate {
                return Err(Error::Conflict);
            }
        } else {
            if self.candidates.len() >= self.capacity {
                return Err(Error::Capacity);
            }
            self.candidates.push(candidate.clone());
        }
        let receipts: Vec<_> = self
            .receipts
            .iter()
            .filter(|r| {
                r.candidate == candidate.version
                    && Some(r.policy) == self.policy.as_ref().map(|p| p.revision)
                    && r.stage == stage
            })
            .cloned()
            .collect();
        let counts = receipts
            .iter()
            .try_fold(Counts::default(), |a, r| qualify::sum(a, r.counts))?;
        let reason = qualify::qualify(self, &candidate, stage, now, &receipts, counts);
        let mut decision = Decision {
            scope: self.scope.clone(),
            candidate: candidate.version,
            policy: self.policy.as_ref().map(|p| p.revision),
            stage,
            authorization: self.policy.clone(),
            at: now,
            receipts: self
                .receipts
                .iter()
                .filter(|r| self.related(r.candidate, &candidate))
                .cloned()
                .collect(),
            counts,
            reason,
            controls: self.controls.clone(),
            transition: Transition::Prepared,
        };
        if let Some((_, receipt)) = &self.fault {
            decision.receipts.push(receipt.clone());
        }
        let generation = if reason == Reason::Qualified {
            Some(Arc::new(Generation {
                origin: self.origin.clone(),
                number: self.next_number()?,
                peers: selected_peers(&candidate, &self.controls),
                snapshot: build(&candidate, &self.controls)?,
                candidate,
                decision: decision.clone(),
            }))
        } else {
            None
        };
        let index = self.decisions.len();
        self.decisions.push(decision.clone());
        Ok(Prepared {
            origin: self.origin.clone(),
            revision: self.revision,
            index,
            decision,
            generation,
        })
    }
    pub fn publish(&mut self, pending: Prepared, now: u64) -> Result<(), Error> {
        self.publish_with(pending, now, |_| Ok(()))
    }
    /// Publisher must be atomic on error, e.g. native Engine::publish_activation.
    /// Replay/compile/publisher failures never replace the current generation.
    pub fn publish_with(
        &mut self,
        pending: Prepared,
        now: u64,
        publisher: impl FnOnce(Arc<Generation>) -> Result<(), Error>,
    ) -> Result<(), Error> {
        if !Arc::ptr_eq(&pending.origin, &self.origin) {
            return Err(Error::Conflict);
        }
        if let Some((reason, _)) = &self.fault {
            self.decisions[pending.index].reason = *reason;
            self.decisions[pending.index].transition = Transition::Conflict;
            return Err(Error::Conflict);
        }
        if pending.revision != self.revision {
            self.decisions[pending.index].transition = Transition::Conflict;
            return Err(Error::Conflict);
        }
        if now < pending.decision.at {
            return Err(Error::Invalid);
        }
        if pending
            .decision
            .authorization
            .as_ref()
            .is_some_and(|p| now >= p.expires)
            || pending.decision.receipts.iter().any(|r| now >= r.expires)
        {
            self.decisions[pending.index].reason = Reason::Expired;
            self.decisions[pending.index].transition = Transition::PublicationFailed;
            return Err(Error::Expired);
        }
        let Some(mut generation) = pending.generation else {
            return Err(Error::Invalid);
        };
        let revision = self.revision.checked_add(1).ok_or(Error::Capacity)?;
        let decision = &mut Arc::get_mut(&mut generation)
            .ok_or(Error::Conflict)?
            .decision;
        decision.transition = Transition::Published;
        decision.at = now;
        if let Err(error) = publisher(generation.clone()) {
            self.decisions[pending.index].transition = Transition::PublicationFailed;
            return Err(error);
        }
        self.decisions[pending.index].transition = Transition::Published;
        self.decisions[pending.index].at = now;
        self.active = Some(generation);
        self.revision = revision;
        Ok(())
    }
    pub fn publish_prepared(
        &mut self,
        candidate: Candidate,
        stage: Stage,
        now: u64,
    ) -> Result<(), Error> {
        let pending = self.prepare(candidate, stage, now)?;
        self.publish(pending, now)
    }
    /// Explicit customer edit, independent of promotion evidence or revoked policy.
    /// Pinning to an unavailable version fails atomically; no silent fallback.
    pub fn reconcile(&mut self) -> Result<(), Error> {
        self.reconcile_with(|_| Ok(()))
    }
    pub fn reconcile_with(
        &mut self,
        publisher: impl FnOnce(Arc<Generation>) -> Result<(), Error>,
    ) -> Result<(), Error> {
        let Some(prior) = &self.active else {
            return Ok(());
        };
        let mut decision = prior.decision.clone();
        decision.controls = self.controls.clone();
        decision.transition = Transition::CustomerEdit;
        let generation = Generation {
            origin: self.origin.clone(),
            number: self.next_number()?,
            peers: selected_peers(&prior.candidate, &self.controls),
            candidate: prior.candidate.clone(),
            snapshot: build(&prior.candidate, &self.controls)?,
            decision,
        };
        let revision = self.revision.checked_add(1).ok_or(Error::Capacity)?;
        let generation = Arc::new(generation);
        publisher(generation.clone())?;
        self.revision = revision;
        self.active = Some(generation);
        Ok(())
    }
    fn related(&self, version: u64, candidate: &Candidate) -> bool {
        version == candidate.version
            || self
                .candidates
                .iter()
                .any(|old| old.version == version && qualify::same_definitions(old, candidate))
    }
    fn next_number(&self) -> Result<u64, Error> {
        self.active
            .as_ref()
            .map_or(Ok(1), |g| g.number.checked_add(1).ok_or(Error::Capacity))
    }
}
fn build(candidate: &Candidate, controls: &Controls) -> Result<store::Snapshot, Error> {
    let mut store = Store::new(Limits {
        scopes: 1,
        versions: 1,
    })
    .map_err(|_| Error::Compilation)?;
    let scope = store::Scope {
        vendor: candidate.scope.vendor.clone(),
        operation: candidate.scope.operation.clone(),
        contract_version: candidate.scope.contract,
    };
    let bindings: Vec<_> = candidate.bindings.iter().map(String::as_str).collect();
    store
        .publish(
            scope.clone(),
            candidate.version,
            &candidate.rules,
            &bindings,
        )
        .map_err(|_| Error::Compilation)?;
    if controls.disabled.iter().any(|id| {
        !candidate.rules.iter().any(|r| &r.id == id) && !candidate.peers.iter().any(|r| &r.id == id)
    }) {
        return Err(Error::Compilation);
    }
    let local = Controls {
        pin: controls.pin,
        overrides: controls.overrides.clone(),
        disabled: controls
            .disabled
            .iter()
            .filter(|id| candidate.rules.iter().any(|r| &r.id == *id))
            .cloned()
            .collect(),
    };
    let mut snapshot = store
        .select(&scope, candidate.version, &local)
        .map_err(|_| Error::Compilation)?;
    snapshot.controls = controls.clone();
    Ok(snapshot)
}
fn selected_peers(candidate: &Candidate, controls: &Controls) -> Vec<crate::cross_endpoint::Rule> {
    candidate
        .peers
        .iter()
        .filter(|r| !controls.disabled.contains(&r.id))
        .cloned()
        .collect()
}
