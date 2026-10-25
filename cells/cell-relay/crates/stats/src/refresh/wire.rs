use super::Error;
use agw_audit::contracts::profile_snapshot::ProfileSnapshot;
use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
const SAFE: u64 = 9_007_199_254_740_991;
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Scope {
    pub tenant: String,
    pub vendor: String,
    pub operation: String,
    pub side: Side,
    pub tenant_segment: String,
    pub volume_cohort: String,
    pub field: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Request,
    Response,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Grain {
    Hour,
    Day,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Selected {
    pub scope: Scope,
    pub generation: String,
    pub grain: Grain,
    pub bucket: String,
    pub start: String,
    pub end: String,
}
fn utc(value: &str) -> Result<DateTime<Utc>, Error> {
    let at = DateTime::parse_from_rfc3339(value)
        .map_err(|_| Error::Invalid)?
        .to_utc();
    if at.to_rfc3339_opts(SecondsFormat::Millis, true) != value {
        return Err(Error::Invalid);
    }
    Ok(at)
}
impl Selected {
    pub fn validate(&self) -> Result<(), Error> {
        let s = &self.scope;
        for part in [
            &s.tenant,
            &s.vendor,
            &s.operation,
            &s.tenant_segment,
            &s.volume_cohort,
            &self.generation,
        ] {
            if part.is_empty() || part.chars().count() > 256 || part.len() > 1024 {
                return Err(Error::Invalid);
            }
        }
        if s.field.len() > 1024 || (!s.field.is_empty() && !s.field.starts_with('/')) {
            return Err(Error::Invalid);
        }
        let mut chars = s.field.chars();
        while let Some(c) = chars.next() {
            if c == '~' && !matches!(chars.next(), Some('0' | '1')) {
                return Err(Error::Invalid);
            }
        }
        let tuple: Vec<String> =
            serde_json::from_str(&s.tenant_segment).map_err(|_| Error::Invalid)?;
        if tuple.len() != 5
            || tuple[0] != "ti48"
            || tuple[2] != self.generation
            || tuple.iter().any(String::is_empty)
            || serde_json::to_string(&tuple).map_err(|_| Error::Invalid)? != s.tenant_segment
        {
            return Err(Error::Invalid);
        }
        let bucket = utc(&self.bucket)?;
        let start = utc(&self.start)?;
        let end = utc(&self.end)?;
        let width = match self.grain {
            Grain::Hour => 3_600_000,
            Grain::Day => 86_400_000,
        };
        if bucket.timestamp_millis().rem_euclid(width) != 0
            || start < bucket
            || end <= start
            || start.timestamp_millis() - bucket.timestamp_millis() >= width
        {
            return Err(Error::Invalid);
        }
        if serde_json::to_vec(self).map_err(|_| Error::Invalid)?.len() > 8192 {
            return Err(Error::Capacity);
        }
        Ok(())
    }
}
/// Immutable aggregate handle. These summaries are not TI-45 exact windows.
#[derive(Debug)]
pub struct Snapshot {
    aggregate: ProfileSnapshot,
    pub(super) canonical: Vec<u8>,
}
impl Snapshot {
    pub fn revision(&self) -> u64 {
        self.aggregate.revision.get()
    }
    pub fn aggregate(&self) -> &ProfileSnapshot {
        &self.aggregate
    }
    /// v1 cannot reconstruct exact enum vocabularies, lossless numeric windows,
    /// timestamps, detector confidence or repair proof. Never mark TI-46 ready.
    pub fn exact_detector_inputs(&self) -> Option<crate::windows::Snapshot> {
        None
    }
    pub(super) fn parse(bytes: &[u8], selected: &Selected) -> Result<Self, Error> {
        if bytes.len() > 131072 {
            return Err(Error::Capacity);
        }
        // Direct typed deserialize rejects duplicate and unknown keys at every boundary.
        let aggregate: ProfileSnapshot =
            serde_json::from_slice(bytes).map_err(|_| Error::Invalid)?;
        let v: Value = serde_json::from_slice(bytes).map_err(|_| Error::Invalid)?;
        let p = &v["profile"];
        if v["generation"] != selected.generation
            || v["grain"] != serde_json::to_value(&selected.grain).map_err(|_| Error::Invalid)?
            || v["bucket"] != selected.bucket
            || p["start"] != selected.start
            || p["end"] != selected.end
            || p["scope"] != serde_json::to_value(&selected.scope).map_err(|_| Error::Invalid)?
            || aggregate.revision.get() > SAFE
            || aggregate.windows.get() > SAFE
        {
            return Err(Error::Invalid);
        }
        let counts = |key: &str| -> Result<Vec<u64>, Error> {
            p[key]
                .as_array()
                .ok_or(Error::Invalid)?
                .iter()
                .map(|n| n.as_u64().filter(|n| *n <= SAFE).ok_or(Error::Invalid))
                .collect()
        };
        let sum = |ns: &[u64]| -> Result<u64, Error> {
            ns.iter().try_fold(0_u64, |a, b| {
                a.checked_add(*b)
                    .filter(|n| *n <= SAFE)
                    .ok_or(Error::Invalid)
            })
        };
        let types = counts("types")?;
        let total = sum(&types)?;
        let scalar = sum(&types[2..6])?;
        let excluded = p["excluded_numbers"]
            .as_u64()
            .filter(|n| *n <= SAFE)
            .ok_or(Error::Invalid)?;
        let hll = counts("hll")?;
        if total == 0
            || sum(&counts("values")?)? != scalar
            || sum(&counts("numeric")?)?.checked_add(excluded) != Some(sum(&types[4..6])?)
            || hll.iter().any(|n| *n > 59)
            || (scalar == 0) != hll.iter().all(|n| *n == 0)
            || (types[3] == 0 && sum(&counts("charset")?)? != 0)
        {
            return Err(Error::Invalid);
        }
        sum(&counts("charset")?)?;
        // BTreeMap-backed Value serialization gives an order-independent fingerprint;
        // timestamp spellings and numeric integer forms were checked above.
        let canonical = serde_json::to_vec(&v).map_err(|_| Error::Invalid)?;
        Ok(Self {
            aggregate,
            canonical,
        })
    }
}
