//! TI-29 append-only local store: immutable records, bounded size, explicit retention.
//!
//! A record is never rewritten. Repetition of the same ID with identical bytes
//! is an idempotent no-op; different bytes under one ID is a conflict. Full
//! capacity or byte ceilings refuse the new record rather than evicting
//! existing evidence. Expiry hides an artifact from retrieval; removal happens
//! only through the explicit [`Store::purge_expired`] call, which a host runs
//! off-path.

use crate::Error;
use crate::contracts::audit_artifact::AuditArtifact;
use chrono::{DateTime, Duration, Utc};
use std::collections::BTreeMap;

/// Default bounded record count.
pub const DEFAULT_CAPACITY: usize = 65_536;
/// Hard ceiling for one local store.
pub const MAX_CAPACITY: usize = 1_000_000;
/// Default cumulative byte ceiling.
pub const DEFAULT_BYTE_LIMIT: usize = 64 * 1024 * 1024;
/// Hard cumulative byte ceiling.
pub const MAX_BYTE_LIMIT: usize = 512 * 1024 * 1024;

/// Store bounds. Retention is measured from the artifact timestamp at append;
/// an artifact already older than retention is refused, not silently kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub capacity: usize,
    pub byte_limit: usize,
    pub retention: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            capacity: DEFAULT_CAPACITY,
            byte_limit: DEFAULT_BYTE_LIMIT,
            retention: Duration::days(90),
        }
    }
}

impl Limits {
    /// Retention floor: one hour. Ceiling: ten years.
    pub fn validate(&self) -> Result<(), Error> {
        if self.capacity == 0 || self.capacity > MAX_CAPACITY {
            return Err(Error::Invalid("store capacity must be 1..=1000000"));
        }
        if self.byte_limit == 0 || self.byte_limit > MAX_BYTE_LIMIT {
            return Err(Error::Invalid("store byte limit must be 1..=536870912"));
        }
        if self.retention < Duration::hours(1) || self.retention > Duration::days(3650) {
            return Err(Error::Invalid(
                "retention must be between 1 hour and 10 years",
            ));
        }
        Ok(())
    }
}

/// The result of an append: a new record or an idempotent repetition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Append {
    Stored,
    Duplicate,
}

#[derive(Debug)]
struct Record {
    artifact: AuditArtifact,
    bytes: Vec<u8>,
    expires_at: DateTime<Utc>,
}

/// One tenant's bounded append-only store.
#[derive(Debug)]
pub struct Store {
    tenant: String,
    limits: Limits,
    records: Vec<Record>,
    index: BTreeMap<String, usize>,
    bytes: usize,
}

impl Store {
    pub fn new(tenant: &str, limits: Limits) -> Result<Self, Error> {
        if !crate::valid_identifier(tenant) {
            return Err(Error::Invalid("store tenant must be a bounded identifier"));
        }
        limits.validate()?;
        Ok(Self {
            tenant: tenant.to_owned(),
            limits,
            records: Vec::new(),
            index: BTreeMap::new(),
            bytes: 0,
        })
    }

    /// Validate and append one artifact.
    pub fn append(&mut self, artifact: AuditArtifact, now: DateTime<Utc>) -> Result<Append, Error> {
        crate::artifact::validate(&artifact)?;
        if let Some(tenant) = artifact
            .metadata
            .get("tenant")
            .and_then(|value| value.as_str())
            && tenant != self.tenant
        {
            return Err(Error::Invalid(
                "artifact tenant metadata does not match the store tenant",
            ));
        }
        let expires_at = artifact.ts + self.limits.retention;
        if expires_at <= now {
            return Err(Error::Expired);
        }
        let bytes = crate::artifact::canonical(&artifact)?;
        let id = crate::text(&artifact.id);
        if let Some(index) = self.index.get(&id).copied() {
            let existing = self
                .records
                .get(index)
                .ok_or(Error::Invalid("store index corrupt"))?;
            return if existing.bytes == bytes {
                Ok(Append::Duplicate)
            } else {
                Err(Error::Conflict)
            };
        }
        if self.records.len() >= self.limits.capacity {
            return Err(Error::Capacity);
        }
        if self.bytes.saturating_add(bytes.len()) > self.limits.byte_limit {
            return Err(Error::ByteLimit);
        }
        self.bytes += bytes.len();
        self.index.insert(id, self.records.len());
        self.records.push(Record {
            artifact,
            bytes,
            expires_at,
        });
        Ok(Append::Stored)
    }

    /// Retrieve a stored artifact unless it is expired. A missing or expired
    /// record is `None`; the caller answers 404, not an internal error.
    pub fn get(&self, id: &str, now: DateTime<Utc>) -> Option<&AuditArtifact> {
        let record = self.records.get(*self.index.get(id)?)?;
        (record.expires_at > now).then_some(&record.artifact)
    }

    /// Whether an ID is tracked, expired or not. Publication uses this to skip
    /// recurrence stamping for an idempotent repetition.
    pub fn contains(&self, id: &str) -> bool {
        self.index.contains_key(id)
    }

    /// Remove every expired record. Returns the number removed; live records
    /// are never touched.
    pub fn purge_expired(&mut self, now: DateTime<Utc>) -> usize {
        let before = self.records.len();
        let mut removed_bytes = 0usize;
        self.records.retain(|record| {
            if record.expires_at <= now {
                removed_bytes += record.bytes.len();
                false
            } else {
                true
            }
        });
        self.bytes -= removed_bytes;
        self.index = self
            .records
            .iter()
            .enumerate()
            .map(|(index, record)| (crate::text(&record.artifact.id), index))
            .collect();
        before - self.records.len()
    }

    /// Enterprise export: every retained artifact in append order. Expired but
    /// unpurged records are included — expiry controls retrieval, not export
    /// of retained evidence.
    pub fn export_all(&self) -> impl Iterator<Item = &AuditArtifact> {
        self.records.iter().map(|record| &record.artifact)
    }

    /// Bounded record count.
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// Whether the store holds nothing.
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Cumulative serialized bytes.
    pub fn bytes(&self) -> usize {
        self.bytes
    }

    /// The tenant this store is bound to.
    pub fn tenant(&self) -> &str {
        &self.tenant
    }

    /// The configured bounds.
    pub fn limits(&self) -> Limits {
        self.limits
    }
}
