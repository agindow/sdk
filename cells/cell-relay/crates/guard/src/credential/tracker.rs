//! Per-binding preallocated history. Identity tokens are copied into fixed
//! slots; the steady path and first-success admission allocate no heap memory.
use super::health::SetupError;
use super::{Cause, Evidence, Signal, classify_complete, complete_evidence};
use std::collections::HashMap;
pub struct Observation<'a> {
    pub operation: &'a str,
    pub arguments: &'a str,
    pub version: Option<&'a str>,
    pub now: u64,
    pub signal: Signal<'a>,
}
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct Text {
    bytes: [u8; 256],
    len: usize,
}
impl Text {
    fn new(value: &str) -> Option<Self> {
        if value.is_empty() || value.len() > 256 {
            return None;
        }
        let mut text = Self {
            bytes: [0; 256],
            len: value.len(),
        };
        text.bytes
            .get_mut(..value.len())?
            .copy_from_slice(value.as_bytes());
        Some(text)
    }
    fn text(&self) -> Option<&str> {
        std::str::from_utf8(self.bytes.get(..self.len)?).ok()
    }
}
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct Key {
    operation: Text,
    arguments: Text,
}
#[derive(Default)]
struct History {
    succeeded: bool,
    failures: u32,
    first: u64,
}
pub struct Tracker {
    cap: usize,
    threshold: u32,
    window: u64,
    version: Option<Text>,
    history: HashMap<Key, History>,
    failed: bool,
    scope_count: u32,
}
impl Tracker {
    pub fn new(cap: usize, threshold: u32, window: u64) -> Result<Self, SetupError> {
        if cap == 0
            || cap > 65536
            || threshold == 0
            || threshold > 1000000
            || window == 0
            || window > 86400000
        {
            return Err(SetupError);
        }
        Ok(Self {
            cap,
            threshold,
            window,
            version: None,
            history: HashMap::with_capacity(cap),
            failed: false,
            scope_count: 0,
        })
    }
    pub fn scope_count(&self) -> u32 {
        self.scope_count
    }
    pub fn observe(&mut self, o: &Observation<'_>) -> Option<(Cause, Evidence)> {
        let (prior, outcome) = self.observe_all(o);
        if prior.is_some() {
            Some((Cause::Rotated, Evidence::HostDeclared))
        } else {
            outcome
        }
    }
    pub fn observe_all(
        &mut self,
        o: &Observation<'_>,
    ) -> (Option<String>, Option<(Cause, Evidence)>) {
        if !complete_evidence(&o.signal) {
            return (None, None);
        }
        let Some(operation) = Text::new(o.operation) else {
            return (None, None);
        };
        let Some(arguments) = Text::new(o.arguments) else {
            return (None, None);
        };
        let version = o.version.and_then(Text::new);
        if o.version.is_some() && version.is_none() {
            return (None, None);
        }
        let prior = if self.version.is_some() && version.is_some() && self.version != version {
            self.version
                .as_ref()
                .and_then(Text::text)
                .map(str::to_owned)
        } else {
            None
        };
        if self.version != version {
            self.version = version;
            self.history.clear();
            self.failed = false;
        }
        let result = self.observe_core(
            o,
            Key {
                operation,
                arguments,
            },
        );
        (prior, result)
    }
    fn observe_core(&mut self, o: &Observation<'_>, key: Key) -> Option<(Cause, Evidence)> {
        if self.history.len() >= self.cap && !self.history.contains_key(&key) {
            let result = classify_complete(&o.signal);
            if result.is_some_and(|(cause, _)| cause.failure()) {
                self.failed = true;
            }
            if (200..300).contains(&o.signal.status) && std::mem::take(&mut self.failed) {
                return Some((Cause::Recovered, Evidence::ProviderDeclared));
            }
            return result;
        }
        let h = self.history.entry(key).or_default();
        if let Some(result) = classify_complete(&o.signal) {
            if result.0.failure() {
                self.failed = true;
            }
            Some(result)
        } else if (200..300).contains(&o.signal.status) {
            h.succeeded = true;
            h.failures = 0;
            if std::mem::take(&mut self.failed) {
                Some((Cause::Recovered, Evidence::ProviderDeclared))
            } else {
                None
            }
        } else if o.signal.status == 403 && h.succeeded {
            if h.failures == 0 || o.now.saturating_sub(h.first) > self.window {
                h.failures = 0;
                h.first = o.now;
            }
            h.failures = h.failures.saturating_add(1).min(1000000);
            self.scope_count = h.failures;
            if h.failures >= self.threshold {
                self.failed = true;
                Some((Cause::ScopeReduced, Evidence::Inferred))
            } else {
                None
            }
        } else {
            None
        }
    }
}
