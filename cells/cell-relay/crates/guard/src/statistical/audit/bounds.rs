//! Composition bounds (TI-47). Every bound is explicit and reported when hit;
//! truncation never drops provenance, changes class or alters policy.
use serde_json::Value;

/// Reserved metadata key for the single composed statistical evidence subtree.
pub const METADATA_KEY: &str = "agindow_statistical";
/// Version of the reserved metadata layout, not the frozen audit schema.
pub const SCHEMA: &str = "agindow_statistical/1";
/// Admitted findings retained per call before truncation.
pub const MAX_FINDINGS: usize = 256;
/// Drift changes retained per call before truncation.
pub const MAX_CHANGES: usize = 1024;
/// Serialized finding budget; provenance and evaluation counters stay.
pub const MAX_FINDING_BYTES: usize = 256 * 1024;

/// Deterministic serialized size; an impossible serialization counts as over.
pub(super) fn json_len(value: &Value) -> usize {
    serde_json::to_vec(value).map_or(usize::MAX, |bytes| bytes.len())
}
