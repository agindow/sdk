use std::collections::VecDeque;

use super::{DeliveryContext, Error, Label, Routing, text};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Admission {
    Enqueued,
    NotNeeded,
    AlreadySubmitted,
    Disabled,
    Full,
    BudgetExhausted,
}

/// One instance per actual call, shared across phases. Costs are integer
/// micro-currency units in the caller's single configured currency/price model.
#[derive(Debug)]
pub struct CallBudget {
    audit_ref: String,
    limit: u64,
    reserved: u64,
    submitted: bool,
    delivery: Option<DeliveryContext>,
}
impl CallBudget {
    pub fn new(audit_ref: &str, limit: u64) -> Result<Self, Error> {
        text(audit_ref)?;
        Ok(Self {
            audit_ref: audit_ref.into(),
            limit,
            reserved: 0,
            submitted: false,
            delivery: None,
        })
    }
    /// Set once before either guard phase. The local-only constructor stays valid.
    pub fn with_delivery(mut self, context: DeliveryContext) -> Result<Self, Error> {
        if self.submitted || self.delivery.is_some() {
            return Err(Error::InvalidInput);
        }
        super::delivery::identifier(&self.audit_ref)?;
        // Revalidate public fields so struct construction cannot bypass the boundary.
        self.delivery = Some(DeliveryContext::new(
            &context.tenant,
            &context.call_id,
            &context.price_context,
            context.created_ms,
        )?);
        Ok(self)
    }
    /// Reserved async upper bound, not billed cost. In-path inference is zero.
    pub fn reserved(&self) -> u64 {
        self.reserved
    }
}

/// Local, non-blocking FIFO. The host owns off-path draining and durable delivery.
/// Capacity bounds queued jobs; the worker must separately bound concurrency.
#[derive(Debug)]
pub struct Outbox {
    capacity: usize,
    jobs: VecDeque<Job>,
}
impl Outbox {
    pub fn new(capacity: usize) -> Result<Self, Error> {
        if capacity > 1024 {
            return Err(Error::ResourceLimit);
        }
        Ok(Self {
            capacity,
            jobs: VecDeque::new(),
        })
    }
    pub fn len(&self) -> usize {
        self.jobs.len()
    }
    pub fn is_empty(&self) -> bool {
        self.jobs.is_empty()
    }
    /// Optional submission never changes the local handlers. No user code is
    /// invoked; charge only after every admission condition has passed.
    pub fn submit(
        &mut self,
        routing: &Routing<'_>,
        call: &mut CallBudget,
        ceiling: u64,
    ) -> Result<Admission, Error> {
        if routing.unavailable().is_some()
            || !routing
                .findings()
                .iter()
                .any(|d| d.label() == Label::Unknown)
        {
            return Ok(Admission::NotNeeded);
        }
        if call.submitted {
            return Ok(Admission::AlreadySubmitted);
        }
        if self.capacity == 0 {
            return Ok(Admission::Disabled);
        }
        if self.len() >= self.capacity {
            return Ok(Admission::Full);
        }
        if ceiling == 0 {
            return Err(Error::InvalidInput);
        }
        if ceiling > call.limit {
            return Ok(Admission::BudgetExhausted);
        }
        self.jobs.push_back(Job {
            audit_ref: call.audit_ref.clone(),
            cap: ceiling,
            delivery: call.delivery.clone(),
            authorized: 0,
        });
        call.reserved = ceiling;
        call.submitted = true;
        Ok(Admission::Enqueued)
    }
    pub fn front(&self) -> Option<&Job> {
        self.jobs.front()
    }
    pub fn pop(&mut self) -> Option<Job> {
        self.jobs.pop_front()
    }
}

/// Non-cloneable work item. All inference, pricing and settlement happen in the
/// background worker, never in `route` or `submit`.
#[derive(Debug)]
pub struct Job {
    audit_ref: String,
    cap: u64,
    authorized: u64,
    delivery: Option<DeliveryContext>,
}
impl Job {
    pub fn delivery(&self) -> Option<&DeliveryContext> {
        self.delivery.as_ref()
    }
    pub fn audit_ref(&self) -> &str {
        &self.audit_ref
    }
    pub fn cap(&self) -> u64 {
        self.cap
    }
    pub fn authorized(&self) -> u64 {
        self.authorized
    }
    /// Reserve the maximum charge BEFORE dispatch. Unsettled/unused grants
    /// remain charged, preventing concurrent work or retries from overspending.
    pub fn authorize(&mut self, maximum: u64) -> Result<Grant, Error> {
        if maximum == 0 {
            return Err(Error::InvalidInput);
        }
        if maximum > self.cap - self.authorized {
            return Err(Error::BudgetExhausted);
        }
        self.authorized += maximum;
        Ok(Grant {
            audit_ref: self.audit_ref.clone(),
            maximum,
        })
    }
}

/// A consumed grant produces one receipt. Workers must enforce provider limits;
/// no local accounting primitive can prevent a provider from overbilling.
#[derive(Debug)]
#[must_use]
pub struct Grant {
    audit_ref: String,
    maximum: u64,
}
impl Grant {
    pub fn settle(self, actual: u64) -> Receipt {
        Receipt {
            audit_ref: self.audit_ref,
            authorized: self.maximum,
            actual,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
#[must_use]
pub struct Receipt {
    pub audit_ref: String,
    pub authorized: u64,
    pub actual: u64,
}
impl Receipt {
    pub fn overrun(&self) -> bool {
        self.actual > self.authorized
    }
}
