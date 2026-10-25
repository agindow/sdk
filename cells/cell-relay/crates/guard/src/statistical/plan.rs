use super::{Call, Operation};
use crate::{config::CallContext, readiness::AvailabilityGate};
use agw_diff::Snapshot;
use agw_stats::{
    control,
    windows::{Availability, Scope},
};
use std::sync::Arc;

/// Scope is supplied by the trusted host, independently of payload content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    pub tenant: String,
    pub connection: String,
    pub vendor: String,
    pub operation: String,
    pub segment: String,
}
#[derive(Debug, Clone)]
pub struct Field {
    pub scope: Scope,
    pub snapshot: Availability,
    /// Only declared categorical fields may run enum novelty.
    pub enumeration: bool,
}
#[derive(Debug, Clone)]
pub struct Plan {
    pub(crate) identity: Identity,
    pub(super) fields: Vec<Field>,
    pub(crate) request_invariants: Option<crate::invariant::Selection>,
    pub(crate) response_invariants: Option<crate::invariant::Selection>,
    pub(crate) cross_endpoint: Option<crate::cross_endpoint::Selection>,
    pub request_shape: Option<Snapshot>,
    pub response_shape: Option<Snapshot>,
    pub alpha: f64,
    pub materiality: f64,
    pub overrides: Vec<Materiality>,
    pub investigation_ceiling: u64,
}
#[derive(Debug, Clone)]
pub struct Materiality {
    pub field: String,
    pub signal: control::Signal,
    pub floor: f64,
}
#[derive(Debug, Clone, Copy)]
pub struct InvalidPlan;
impl std::fmt::Display for InvalidPlan {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid statistical plan")
    }
}
impl std::error::Error for InvalidPlan {}
impl Plan {
    pub fn new(identity: Identity, fields: Vec<Field>) -> Result<Self, InvalidPlan> {
        if fields.len() > 7
            || [
                &identity.tenant,
                &identity.connection,
                &identity.vendor,
                &identity.operation,
                &identity.segment,
            ]
            .iter()
            .any(|s| s.is_empty() || s.len() > 1024)
        {
            return Err(InvalidPlan);
        }
        for (i, field) in fields.iter().enumerate() {
            let s = &field.scope;
            if s.tenant != identity.tenant
                || s.connection != identity.connection
                || s.vendor != identity.vendor
                || s.operation != identity.operation
                || s.segment != identity.segment
                || s.field.is_empty()
                || s.field.len() > 1024
                || s.window.is_empty()
                || s.generation.is_empty()
                || s.window.len() > 1024
                || s.generation.len() > 1024
                || fields[..i]
                    .iter()
                    .any(|p| p.scope.side == s.side && p.scope.field == s.field)
                || matches!(&field.snapshot, Availability::Ready(snapshot) if snapshot.scope()!=s)
            {
                return Err(InvalidPlan);
            }
        }
        Ok(Self {
            identity,
            fields,
            request_invariants: None,
            response_invariants: None,
            cross_endpoint: None,
            request_shape: None,
            response_shape: None,
            alpha: 0.05,
            materiality: 0.2,
            overrides: Vec::new(),
            investigation_ceiling: 1,
        })
    }
    pub fn with_invariants(
        mut self,
        side: agw_diff::Side,
        selection: crate::invariant::Selection,
    ) -> Result<Self, InvalidPlan> {
        if selection.activation.as_ref().is_some_and(|g| {
            let scope = &g.decision().scope;
            scope.tenant != self.identity.tenant || scope.connection != self.identity.connection
        }) || selection.snapshot.scope.vendor != self.identity.vendor
            || selection.snapshot.scope.operation != self.identity.operation
        {
            return Err(InvalidPlan);
        }
        if self
            .cross_endpoint
            .as_ref()
            .is_some_and(|cross| cross.conflicts(&selection))
        {
            return Err(InvalidPlan);
        }
        let target = if side == agw_diff::Side::Request {
            &mut self.request_invariants
        } else {
            &mut self.response_invariants
        };
        if target.is_some() {
            return Err(InvalidPlan);
        }
        *target = Some(selection);
        Ok(self)
    }
    pub fn with_cross_endpoint(
        mut self,
        selection: crate::cross_endpoint::Selection,
    ) -> Result<Self, InvalidPlan> {
        if self.cross_endpoint.is_some()
            || !selection.valid_for(&self.identity.operation)
            || [&self.request_invariants, &self.response_invariants]
                .iter()
                .any(|local| {
                    local
                        .as_ref()
                        .is_some_and(|local| selection.conflicts(local))
                })
        {
            return Err(InvalidPlan);
        }
        self.cross_endpoint = Some(selection);
        Ok(self)
    }
    pub(crate) fn invariants(&self, side: agw_diff::Side) -> Option<&crate::invariant::Selection> {
        if side == agw_diff::Side::Request {
            self.request_invariants.as_ref()
        } else {
            self.response_invariants.as_ref()
        }
    }
    pub fn family(&self) -> usize {
        self.fields.len() * 535
    }
}
/// Default construction never requires a baseline or enables uncalibrated work.
#[derive(Default)]
pub enum Engine {
    #[default]
    NullEngine,
    Statistical {
        plan: Arc<Plan>,
        gate: AvailabilityGate,
    },
}
impl Engine {
    pub fn statistical(plan: Plan) -> Self {
        Self::Statistical {
            plan: Arc::new(plan),
            gate: AvailabilityGate::new(true),
        }
    }
    pub fn replace(&mut self, plan: Plan) {
        match self {
            Self::Statistical { plan: prior, .. } => *prior = Arc::new(plan),
            Self::NullEngine => *self = Self::statistical(plan),
        }
    }
    /// Background publication only. Clone/validate before the single Arc swap;
    /// calls already begun retain the prior plan and readiness breaker.
    pub fn publish_activation(
        &mut self,
        side: agw_diff::Side,
        generation: Arc<agw_invariants::activation::Generation>,
    ) -> Result<(), InvalidPlan> {
        let Self::Statistical { plan, .. } = self else {
            return Err(InvalidPlan);
        };
        let mut next = (**plan).clone();
        let target = if side == agw_diff::Side::Request {
            &mut next.request_invariants
        } else {
            &mut next.response_invariants
        };
        if target.as_ref().is_some_and(|prior| {
            prior.activation.as_ref().is_none_or(|old| {
                !old.same_controller(&generation)
                    || old.number() >= generation.number()
                    || old.decision().scope != generation.decision().scope
            })
        }) {
            return Err(InvalidPlan);
        }
        *target = None;
        let prior_cross = next
            .cross_endpoint
            .as_ref()
            .and_then(|s| s.activation.as_ref());
        let same_cross = prior_cross.is_some_and(|old| Arc::ptr_eq(old, &generation));
        let owns_cross = prior_cross.is_some_and(|old| old.same_controller(&generation));
        if !same_cross && (generation.has_peers() || owns_cross) {
            if next.cross_endpoint.as_ref().is_some_and(|prior| {
                prior.activation.as_ref().is_none_or(|old| {
                    !old.same_controller(&generation)
                        || old.number() >= generation.number()
                        || old.decision().scope != generation.decision().scope
                })
            }) {
                return Err(InvalidPlan);
            }
            next.cross_endpoint = None;
            if generation.has_peers() {
                next = next.with_cross_endpoint(crate::cross_endpoint::Selection::activated(
                    generation.clone(),
                )?)?;
            }
        }
        let next =
            next.with_invariants(side, crate::invariant::Selection::activated(generation)?)?;
        *plan = Arc::new(next);
        Ok(())
    }
    pub fn set_ready(&self, ready: bool) {
        if let Self::Statistical { gate, .. } = self {
            gate.set_ready(ready)
        }
    }
    pub fn begin_call<'a>(
        &self,
        context: CallContext,
        operation: Operation,
        audit_ref: &'a str,
        work: usize,
        inference: u64,
    ) -> Call<'a> {
        let (plan, gate) = match self {
            Self::NullEngine => (None, None),
            Self::Statistical { plan, gate } => (Some(Arc::clone(plan)), Some(gate.clone())),
        };
        Call {
            context,
            operation,
            audit_ref,
            work,
            inference,
            delivery: None,
            plan,
            gate,
            workflow: None,
        }
    }
}
