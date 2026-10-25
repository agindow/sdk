//! TI-58: bounded guest-only FIFO; the host owns validation and delivery.
use agw_guard::statistical::Outcome;
use chrono::{DateTime, Utc};
use serde_json::json;
use std::{cell::RefCell, collections::VecDeque, rc::Rc};

pub type Shared = Rc<RefCell<Outbox>>;
#[derive(Default)]
pub struct Outbox {
    pub enabled: bool,
    tenant: String,
    namespace: String,
    sequence: u64,
    capacity: usize,
    max_bytes: usize,
    bytes: usize,
    dropped: u64,
    queue: VecDeque<(String, String)>,
}
pub struct Context {
    pub tool: String,
    pub ts: DateTime<Utc>,
}
impl Outbox {
    pub fn enable(
        &mut self,
        tenant: &str,
        namespace: &str,
        capacity: u32,
        max_bytes: u32,
    ) -> Result<(), String> {
        if self.enabled
            || tenant.is_empty()
            || tenant.len() > 256
            || tenant.chars().any(char::is_control)
            || namespace.is_empty()
            || namespace.len() > 64
            || !namespace
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-')
            || !(1..=256).contains(&capacity)
            || !(1024..=1_048_576).contains(&max_bytes)
        {
            return Err("invalid audit setup".into());
        }
        self.enabled = true;
        self.tenant = tenant.into();
        self.namespace = namespace.into();
        self.capacity = capacity as usize;
        self.max_bytes = max_bytes as usize;
        Ok(())
    }
    pub fn publish(&mut self, outcome: &Outcome<()>, context: Option<Context>) {
        if !self.enabled {
            return;
        }
        let Some(context) = context else {
            self.drop_one();
            return;
        };
        let Some(sequence) = self.sequence.checked_add(1) else {
            self.drop_one();
            return;
        };
        self.sequence = sequence;
        if self.queue.len() >= self.capacity {
            self.drop_one();
            return;
        }
        let id = format!("aud_{}_{}", self.namespace, sequence);
        let draft = serde_json::from_value(json!({
            "id": id, "ts": context.ts, "tool": context.tool,
            "operation": "read", "mode": "observe", "drift": null, "decision": "ok", "patch": null,
            "proof": { "contract_version": "unavailable", "verified_against": "cache", "write_integrity": false },
            "metadata": { "tenant": self.tenant, "verification": "no contract verification claimed by embedded drain" }
        }));
        let Ok(draft) = draft else {
            self.drop_one();
            return;
        };
        let artifact = outcome.compose(draft);
        if agw_audit::artifact::validate(&artifact).is_err() {
            self.drop_one();
            return;
        }
        let Ok(json) = serde_json::to_string(&artifact) else {
            self.drop_one();
            return;
        };
        if json.len() > 131072 || self.bytes.saturating_add(json.len()) > self.max_bytes {
            self.drop_one();
            return;
        }
        self.bytes += json.len();
        self.queue.push_back((id, json));
    }
    fn drop_one(&mut self) {
        self.dropped = self.dropped.saturating_add(1);
    }
    pub fn peek(&self) -> String {
        self.queue
            .front()
            .map_or_else(|| "null".into(), |(_, bytes)| bytes.clone())
    }
    pub fn ack(&mut self, id: &str) -> bool {
        if self.queue.front().is_none_or(|(head, _)| head != id) {
            return false;
        }
        if let Some((_, bytes)) = self.queue.pop_front() {
            self.bytes -= bytes.len();
        }
        true
    }
    pub fn stats(&self) -> String {
        json!({ "pending": self.queue.len(), "dropped": self.dropped,
        "bytes": self.bytes, "enabled": self.enabled })
        .to_string()
    }
}
