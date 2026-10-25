//! Tenant-owned projection with O(1) steady-state reads/updates. New-record
//! admission scans at most the configured cap. Host-assigned monotonic sequence
//! identities per binding prevent stale/duplicate deliveries regressing records.
use super::{Cause, Evidence};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
#[derive(Debug)]
pub struct SetupError;
impl std::fmt::Display for SetupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("invalid credential bounds")
    }
}
impl std::error::Error for SetupError {}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Health {
    Healthy,
    Degrading,
    Failing,
    Superseded,
    Unknown,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Record {
    pub health: Health,
    pub last_cause: Cause,
    pub first_fail: Option<u64>,
    pub last_fail: Option<u64>,
    pub last_ok: Option<u64>,
    pub revision: u64,
    pub observed_since: u64,
    failures: u32,
    sequence: u64,
    expires_at: Option<u64>,
    warned: bool,
    superseded_at: Option<u64>,
}
#[derive(Clone, Copy)]
pub struct Update<'a> {
    pub tenant: &'a str,
    pub connection: &'a str,
    pub version: Option<&'a str>,
    pub prior_version: Option<&'a str>,
    pub cause: Cause,
    pub evidence: Evidence,
    pub now: u64,
    pub sequence: u64,
}
pub struct Projection {
    tenant: String,
    cap: usize,
    threshold: u32,
    retention: u64,
    dropped: u64,
    records: HashMap<String, HashMap<String, Record>>,
    count: usize,
}
impl Projection {
    pub fn with_defaults(tenant: &str) -> Result<Self, SetupError> {
        Self::new(tenant, 4096, 3, 300000)
    }

    pub fn new(
        tenant: &str,
        cap: usize,
        threshold: u32,
        retention: u64,
    ) -> Result<Self, SetupError> {
        if tenant.is_empty() || tenant.len() > 256 || cap == 0 || threshold == 0 || retention == 0 {
            return Err(SetupError);
        }
        Ok(Self {
            tenant: tenant.to_owned(),
            cap,
            threshold,
            retention,
            dropped: 0,
            records: HashMap::new(),
            count: 0,
        })
    }
    pub fn len(&self) -> usize {
        self.count
    }
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }
    pub fn dropped(&self) -> u64 {
        self.dropped
    }
    pub fn get(&self, connection: &str, version: Option<&str>) -> Option<&Record> {
        self.records.get(connection)?.get(version.unwrap_or(""))
    }
    /// Canonicalize a bounded timestamp window in the host's existing buffer.
    /// Sorting is in-place and never delays or replays an upstream dispatch.
    pub fn apply_window(&mut self, updates: &mut [Update<'_>]) -> bool {
        if updates.len() > self.cap.saturating_mul(10)
            || updates.iter().any(|u| u.tenant != self.tenant)
        {
            return false;
        }
        updates.sort_unstable_by(|a, b| {
            (a.now, a.sequence, a.connection, a.version).cmp(&(
                b.now,
                b.sequence,
                b.connection,
                b.version,
            ))
        });
        for update in updates {
            self.apply(update);
        }
        true
    }
    pub fn apply(&mut self, u: &Update<'_>) -> bool {
        if u.tenant != self.tenant
            || u.connection.is_empty()
            || u.connection.len() > 256
            || u.version.is_some_and(|v| v.is_empty() || v.len() > 256)
        {
            return false;
        }
        if self
            .get(u.connection, u.version)
            .is_some_and(|r| r.sequence >= u.sequence)
        {
            return false;
        }
        if u.cause == Cause::Rotated
            && let Some(old) = self
                .records
                .get_mut(u.connection)
                .and_then(|versions| versions.get_mut(u.prior_version.unwrap_or("")))
        {
            old.health = Health::Superseded;
            old.superseded_at = Some(u.now);
            old.revision = old.revision.saturating_add(1);
        }
        if self.get(u.connection, u.version).is_none() {
            self.evict(u.now);
            let r = Record {
                health: Health::Unknown,
                last_cause: Cause::Unknown,
                first_fail: None,
                last_fail: None,
                last_ok: None,
                revision: 0,
                observed_since: u.now,
                failures: 0,
                sequence: 0,
                expires_at: None,
                warned: false,
                superseded_at: None,
            };
            self.records
                .entry(u.connection.to_owned())
                .or_default()
                .insert(u.version.unwrap_or("").to_owned(), r);
            self.count += 1;
        }
        let Some(r) = self
            .records
            .get_mut(u.connection)
            .and_then(|v| v.get_mut(u.version.unwrap_or("")))
        else {
            return false;
        };
        let recovered =
            u.cause == Cause::Recovered && matches!(r.health, Health::Degrading | Health::Failing);
        r.sequence = u.sequence;
        r.revision = r.revision.saturating_add(1);
        r.last_cause = u.cause;
        if r.health == Health::Superseded {
            if u.cause.failure() {
                r.first_fail.get_or_insert(u.now);
                r.last_fail = Some(u.now);
            } else if u.cause == Cause::Recovered {
                r.last_ok = Some(u.now);
            }
            return false;
        }
        if u.cause.failure() {
            r.failures = r.failures.saturating_add(1);
            r.first_fail.get_or_insert(u.now);
            r.last_fail = Some(u.now);
            r.health = if r.failures >= self.threshold || u.cause.terminal(u.evidence) {
                Health::Failing
            } else {
                Health::Degrading
            };
        } else if u.cause == Cause::Rotated {
            r.health = Health::Degrading;
        } else if u.cause == Cause::Recovered {
            r.health = Health::Healthy;
            r.last_ok = Some(u.now);
            r.failures = 0;
            r.first_fail = None;
        }
        recovered
    }
    /// Raw successful responses update liveness without inventing an emitted
    /// credential event. The return value identifies one actual failure recovery.
    pub fn success(
        &mut self,
        tenant: &str,
        connection: &str,
        version: Option<&str>,
        now: u64,
        sequence: u64,
    ) -> bool {
        if tenant != self.tenant {
            return false;
        }
        let prior = self
            .get(connection, version)
            .map(|r| (r.last_cause, r.failures));
        let recovered = self.apply(&Update {
            tenant,
            connection,
            version,
            prior_version: None,
            cause: Cause::Recovered,
            evidence: Evidence::ProviderDeclared,
            now,
            sequence,
        });
        if let Some(r) = self
            .records
            .get_mut(connection)
            .and_then(|v| v.get_mut(version.unwrap_or("")))
            && r.sequence == sequence
            && prior.is_none_or(|(_, failures)| failures == 0)
        {
            r.last_cause = prior.map_or(Cause::Unknown, |(cause, _)| cause);
        }
        recovered && prior.is_some_and(|(_, failures)| failures > 0)
    }
    fn evict(&mut self, now: u64) {
        for versions in self.records.values_mut() {
            versions.retain(|_, r| {
                r.health != Health::Superseded
                    || now.saturating_sub(r.superseded_at.unwrap_or(now)) < self.retention
            });
        }
        self.records.retain(|_, v| !v.is_empty());
        self.count = self.records.values().map(HashMap::len).sum();
        if self.count < self.cap {
            return;
        }
        let oldest = self
            .records
            .iter()
            .flat_map(|(c, v)| {
                v.iter()
                    .map(move |(v, r)| (c.clone(), v.clone(), r.observed_since))
            })
            .min_by(|a, b| (a.2, &a.0, &a.1).cmp(&(b.2, &b.0, &b.1)));
        if let Some((c, v, _)) = oldest {
            if let Some(versions) = self.records.get_mut(&c) {
                versions.remove(&v);
            }
            self.records.retain(|_, v| !v.is_empty());
            self.count -= 1;
            self.dropped = self.dropped.saturating_add(1);
        }
    }
    /// Host-provided expiry only. Ticks return one signal per version.
    pub fn expiry(
        &mut self,
        connection: &str,
        version: Option<&str>,
        expires_at: u64,
        now: u64,
        warning: u64,
    ) -> bool {
        let Some(r) = self
            .records
            .get_mut(connection)
            .and_then(|v| v.get_mut(version.unwrap_or("")))
        else {
            return false;
        };
        if r.expires_at != Some(expires_at) {
            r.expires_at = Some(expires_at);
            r.warned = false;
        }
        if expires_at.saturating_sub(now) <= warning && !r.warned {
            r.warned = true;
            return true;
        }
        false
    }
}
