//! TII-25 native background delivery. Volatile ownership is explicit: only a
//! hosted 202 is durable acceptance. Never pump while holding the guard's outbox.
use crate::router::{Error, Job, Outbox};
use chrono::{DateTime, SecondsFormat};
use sha2::{Digest, Sha256};
use std::collections::VecDeque;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Accepted,
    Retry,
    Uncertain,
    Rejected,
}
#[derive(Debug, Clone, Default)]
pub struct Counters {
    pub delivered: u64,
    pub retried: u64,
    pub dropped: u64,
    pub expired: u64,
    pub dead_letter: u64,
}
#[derive(Debug, Clone)]
pub struct Terminal {
    pub id: String,
    pub audit_ref: String,
    pub status: &'static str,
}
struct Entry {
    body: Vec<u8>,
    id: String,
    audit_ref: String,
    expires: i64,
    attempts: u8,
    next: i64,
    uncertain: bool,
}
/// At most 1024 * 4096 payload bytes plus fixed bounded metadata; one sender
/// invocation per pump. The host runs this single owner on a background thread.
pub struct Exporter {
    capacity: usize,
    entries: VecDeque<Entry>,
    terminal: VecDeque<Terminal>,
    counters: Counters,
}
impl Exporter {
    pub fn new(capacity: usize) -> Result<Self, Error> {
        if capacity > 1024 {
            return Err(Error::ResourceLimit);
        }
        Ok(Self {
            capacity,
            entries: VecDeque::new(),
            terminal: VecDeque::new(),
            counters: Counters::default(),
        })
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub fn counters(&self) -> &Counters {
        &self.counters
    }
    pub fn terminal(&self) -> &VecDeque<Terminal> {
        &self.terminal
    }
    /// A full exporter leaves the FIFO untouched. Invalid/local-spent work is
    /// consumed as a visible drop, never reauthorized under another ledger.
    pub fn collect_one(&mut self, source: &mut Outbox) -> Result<bool, Error> {
        if self.entries.len() >= self.capacity {
            return Ok(false);
        }
        let Some(job) = source.pop() else {
            return Ok(false);
        };
        self.stage(job)?;
        Ok(true)
    }
    pub fn stage(&mut self, job: Job) -> Result<(), Error> {
        if self.entries.len() >= self.capacity {
            return Err(Error::ResourceLimit);
        }
        let entry = match project(&job) {
            Ok(entry) => entry,
            Err(error) => {
                self.counters.dropped = self.counters.dropped.saturating_add(1);
                self.record(Terminal {
                    id: String::new(),
                    audit_ref: job.audit_ref().into(),
                    status: "invalid-local-drop",
                });
                return Err(error);
            }
        };
        if let Some(prior) = self.entries.iter().find(|prior| prior.id == entry.id) {
            return if prior.body == entry.body {
                Ok(())
            } else {
                Err(Error::InvalidInput)
            };
        }
        self.entries.push_back(entry);
        Ok(())
    }
    /// The reused native HTTPS transport sends credentials only on stdin,
    /// disables redirects and enforces a five-second investigation deadline.
    pub fn pump_https(&mut self, now: i64, sender: &crate::export::native::HttpsSender) {
        self.pump(now, |bytes| match sender.send(bytes) {
            Ok(true) => Outcome::Accepted,
            _ => Outcome::Uncertain,
        });
    }
    pub fn pump(&mut self, now: i64, mut send: impl FnMut(&[u8]) -> Outcome) {
        let Some(mut entry) = self.entries.pop_front() else {
            return;
        };
        if now >= entry.expires {
            self.counters.expired = self.counters.expired.saturating_add(1);
            self.end(entry, "expired");
            return;
        }
        if now < entry.next {
            self.entries.push_front(entry);
            return;
        }
        entry.attempts += 1;
        match send(&entry.body) {
            Outcome::Accepted => {
                self.counters.delivered = self.counters.delivered.saturating_add(1);
                self.end(entry, "durable-accepted");
            }
            Outcome::Rejected => {
                self.counters.dead_letter = self.counters.dead_letter.saturating_add(1);
                self.end(entry, "rejected");
            }
            outcome => {
                entry.uncertain |= outcome == Outcome::Uncertain;
                if entry.attempts >= 8 {
                    self.counters.dead_letter = self.counters.dead_letter.saturating_add(1);
                    self.end(entry, "attempts-exhausted");
                } else {
                    self.counters.retried = self.counters.retried.saturating_add(1);
                    entry.next = now.saturating_add((1000_i64 << (entry.attempts - 1)).min(30000));
                    self.entries.push_back(entry);
                }
            }
        }
    }
    /// Graceful shutdown can report losses. A process crash cannot report or
    /// recover this volatile FIFO; operators must inspect durable hosted state.
    pub fn shutdown(&mut self) -> Vec<Terminal> {
        let mut losses = Vec::new();
        while let Some(entry) = self.entries.pop_front() {
            let record = Terminal {
                id: entry.id,
                audit_ref: entry.audit_ref,
                status: if entry.uncertain {
                    "uncertain-local-loss"
                } else {
                    "local-only-loss"
                },
            };
            self.counters.dropped = self.counters.dropped.saturating_add(1);
            self.record(record.clone());
            losses.push(record);
        }
        losses
    }
    fn end(&mut self, entry: Entry, status: &'static str) {
        self.record(Terminal {
            id: entry.id,
            audit_ref: entry.audit_ref,
            status,
        });
    }
    fn record(&mut self, record: Terminal) {
        if self.terminal.len() >= 256 {
            self.terminal.pop_front();
        }
        self.terminal.push_back(record);
    }
}
fn project(job: &Job) -> Result<Entry, Error> {
    if job.authorized() != 0 {
        return Err(Error::BudgetExhausted);
    }
    let context = job.delivery().ok_or(Error::InvalidInput)?;
    let tuple = serde_json::to_vec(&[
        context.tenant.as_str(),
        context.call_id.as_str(),
        job.audit_ref(),
    ])
    .map_err(|_| Error::InvalidInput)?;
    let id = format!("inv_{:x}", Sha256::digest(tuple));
    let expires = context
        .created_ms
        .checked_add(600000)
        .ok_or(Error::InvalidInput)?;
    let stamp = |ms| {
        DateTime::from_timestamp_millis(ms)
            .map(|time| time.to_rfc3339_opts(SecondsFormat::Millis, true))
            .ok_or(Error::InvalidInput)
    };
    let body = serde_json::to_vec(&serde_json::json!({ "contract": "investigation_job", "version": 1, "id": id,
        "tenant": context.tenant, "call_id": context.call_id, "audit_id": job.audit_ref(), "ceiling": job.cap().to_string(),
        "currency": "USD", "price_context": context.price_context, "created_at": stamp(context.created_ms)?, "expires_at": stamp(expires)?
    })).map_err(|_| Error::InvalidInput)?;
    agw_audit::investigation_wire::validate(&body).map_err(|_| Error::InvalidInput)?;
    Ok(Entry {
        body,
        id,
        audit_ref: job.audit_ref().into(),
        expires,
        attempts: 0,
        next: context.created_ms,
        uncertain: false,
    })
}
