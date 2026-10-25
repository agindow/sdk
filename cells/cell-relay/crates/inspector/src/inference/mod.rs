//! TI-10: bounded spec-optional learning and local immutable baseline history.
//!
//! This is a library seam, not an engine decision path. Call after capture on
//! complete response bytes, outside forwarding. Map learning failures to bypass
//! at the caller's availability boundary. No alerts, I/O or payload rewriting.
mod schema;
mod shape;

use std::collections::BTreeMap;
use std::fmt;
use std::num::NonZeroUsize;

use schema::Evidence;
pub use schema::{JsonType, Schema, SpecPrior};

/// Exact response stream identity. Tenant/cohort segmentation can be encoded
/// by the caller in the vendor/operation identity; no global fallback exists.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Key {
    pub vendor: String,
    pub operation: String,
}

impl Key {
    pub fn new(vendor: impl Into<String>, operation: impl Into<String>) -> Self {
        Self {
            vendor: vendor.into(),
            operation: operation.into(),
        }
    }
}

/// Independent resource ceilings. Zero denies that resource; no eviction.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub max_bytes: usize,
    /// Root depth is zero.
    pub max_depth: usize,
    /// Both parsed nodes per sample and accumulated schema nodes.
    pub max_nodes: usize,
    pub max_keys: usize,
    /// Includes a learning version, per key.
    pub max_versions: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_bytes: crate::DEFAULT_CAP,
            max_depth: 64,
            max_nodes: 4096,
            max_keys: 128,
            max_versions: 32,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Limit {
    Bytes,
    Depth,
    Nodes,
    Keys,
    Versions,
}

/// Recorded learning failures; these are never drift verdicts or policy blocks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LearnError {
    Truncated,
    Malformed(String),
    Limit(Limit),
    NotStarted,
    AlreadyLearning,
    VersionOverflow,
}

impl fmt::Display for LearnError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "schema learning: {self:?}")
    }
}
impl std::error::Error for LearnError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Progress {
    Warming { samples: usize, required: usize },
    Established { version: u64 },
    Frozen { version: u64 },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Baseline {
    pub version: u64,
    /// Live samples; excludes the optional prior.
    pub samples: usize,
    pub schema: Schema,
}

struct Learning {
    version: u64,
    samples: usize,
    prior_samples: usize,
    evidence: Evidence,
}

#[derive(Default)]
struct History {
    versions: Vec<Baseline>,
    learning: Option<Learning>,
}

/// In-memory ownership only. No locks, background work or synchronous services.
pub struct BaselineStore {
    required: NonZeroUsize,
    limits: Limits,
    histories: BTreeMap<Key, History>,
}

impl BaselineStore {
    pub fn new(required: NonZeroUsize, limits: Limits) -> Self {
        Self {
            required,
            limits,
            histories: BTreeMap::new(),
        }
    }

    /// Start an explicit learning generation; preserve all published versions.
    /// Reject restarting an unfinished generation instead of discarding samples.
    pub fn begin(&mut self, key: Key, prior: Option<SpecPrior>) -> Result<u64, LearnError> {
        let history = self.histories.get(&key);
        if history.is_some_and(|h| h.learning.is_some()) {
            return Err(LearnError::AlreadyLearning);
        }
        if history.is_none() && self.histories.len() >= self.limits.max_keys {
            return Err(LearnError::Limit(Limit::Keys));
        }
        if history.map_or(0, |h| h.versions.len()) >= self.limits.max_versions {
            return Err(LearnError::Limit(Limit::Versions));
        }
        let version = history
            .and_then(|h| h.versions.last())
            .map_or(Some(1), |b| b.version.checked_add(1))
            .ok_or(LearnError::VersionOverflow)?;
        let prior_samples = usize::from(prior.is_some());
        let mut evidence = Evidence::default();
        if let Some(prior) = prior {
            shape::validate_prior(&prior, 0, self.limits)?;
            evidence.add(prior);
        }
        self.histories.entry(key).or_default().learning = Some(Learning {
            version,
            samples: 0,
            prior_samples,
            evidence,
        });
        Ok(version)
    }

    /// Accept a complete captured response. Frozen versions skip parsing entirely.
    /// Validate and merge on a candidate so failures consume no samples or state.
    pub fn observe(
        &mut self,
        key: &Key,
        captured: &[u8],
        truncated: bool,
    ) -> Result<Progress, LearnError> {
        let history = self.histories.get_mut(key).ok_or(LearnError::NotStarted)?;
        let Some(learning) = history.learning.as_mut() else {
            return history
                .versions
                .last()
                .map(|b| Progress::Frozen { version: b.version })
                .ok_or(LearnError::NotStarted);
        };
        if truncated {
            return Err(LearnError::Truncated);
        }
        let shape = shape::parse(captured, self.limits)?;
        let mut candidate = learning.evidence.clone();
        candidate.add(shape);
        if candidate.nodes() > self.limits.max_nodes {
            return Err(LearnError::Limit(Limit::Nodes));
        }
        learning.evidence = candidate;
        learning.samples += 1;
        if learning.samples < self.required.get() {
            return Ok(Progress::Warming {
                samples: learning.samples,
                required: self.required.get(),
            });
        }
        let version = learning.version;
        history.versions.push(Baseline {
            version,
            samples: learning.samples,
            schema: learning
                .evidence
                .schema(learning.samples + learning.prior_samples),
        });
        history.learning = None;
        Ok(Progress::Established { version })
    }

    pub fn latest(&self, key: &Key) -> Option<&Baseline> {
        self.histories.get(key)?.versions.last()
    }

    pub fn version(&self, key: &Key, version: u64) -> Option<&Baseline> {
        self.histories
            .get(key)?
            .versions
            .iter()
            .find(|b| b.version == version)
    }
}
