use super::{Error, ReviewCase, Scope, identifier};
use crate::{gated::Outcome, idempotency::Completion};
use std::collections::VecDeque;
struct Pending {
    value: ReviewCase,
    bytes: Vec<u8>,
}
/// Caller-owned off-path handoff. Retain the front until delivery acknowledges it.
pub struct Outbox {
    tenant: String,
    capacity: usize,
    byte_limit: usize,
    bytes: usize,
    pending: VecDeque<Pending>,
}
impl Outbox {
    pub fn new(tenant: &str, capacity: usize, byte_limit: usize) -> Result<Self, Error> {
        if !identifier(tenant)
            || capacity == 0
            || capacity > 128
            || byte_limit == 0
            || byte_limit > 8 * 1024 * 1024
        {
            return Err(Error::InvalidScope);
        }
        Ok(Self {
            tenant: tenant.into(),
            capacity,
            byte_limit,
            bytes: 0,
            pending: VecDeque::new(),
        })
    }
    pub fn submit<O>(&mut self, outcome: &Outcome<'_, O>, scope: &Scope) -> Result<bool, Error> {
        match super::case(outcome, scope)? {
            Some(value) => self.push(value),
            None => Ok(false),
        }
    }
    pub fn submit_completion<O>(
        &mut self,
        outcome: &Completion<O>,
        scope: &Scope,
    ) -> Result<bool, Error> {
        match super::case_from_completion(outcome, scope)? {
            Some(value) => self.push(value),
            None => Ok(false),
        }
    }
    fn push(&mut self, value: ReviewCase) -> Result<bool, Error> {
        if value.scope.tenant.as_str() != self.tenant {
            return Err(Error::InvalidScope);
        }
        let bytes = serde_json::to_vec(&value).map_err(|_| Error::InvalidEnvelope)?;
        if let Some(existing) = self.pending.iter().find(|item| item.value.id == value.id) {
            return if existing.bytes == bytes {
                Ok(false)
            } else {
                Err(Error::Conflict)
            };
        }
        if self.pending.len() >= self.capacity
            || self.bytes.saturating_add(bytes.len()) > self.byte_limit
        {
            return Err(Error::Full);
        }
        self.bytes += bytes.len();
        self.pending.push_back(Pending { value, bytes });
        Ok(true)
    }
    pub fn front(&self) -> Option<&ReviewCase> {
        self.pending.front().map(|value| &value.value)
    }
    pub fn ack(&mut self, id: &str) -> Result<(), Error> {
        if self.front().is_none_or(|value| value.id.as_str() != id) {
            return Err(Error::UnexpectedAck);
        }
        if let Some(value) = self.pending.pop_front() {
            self.bytes -= value.bytes.len();
        }
        Ok(())
    }
}
