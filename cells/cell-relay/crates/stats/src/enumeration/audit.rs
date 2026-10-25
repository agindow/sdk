//! Generated v1 audit draft handoff. No publication or final policy decision.
use agw_audit::contracts::audit_artifact::{AuditArtifact, Change, Drift, DriftClass, DriftSide};
use agw_diff::Side;
use serde_json::json;

use super::{Error, Evidence, Report};

impl Report {
    /// Only material semantic evidence may stamp drift. Final policy remains
    /// caller-owned; stale repair and integrity claims cannot survive.
    pub fn audit(&self, mut draft: AuditArtifact, side: Side) -> Result<AuditArtifact, Error> {
        if self.evidence != Evidence::Semantic {
            return Err(Error::NoFinding);
        }
        let first_seen = self
            .new_values
            .iter()
            .filter(|v| v.material)
            .map(|v| v.first_seen)
            .min()
            .ok_or(Error::NoFinding)?;
        let changes = self
            .new_values
            .iter()
            .filter(|v| v.material)
            .map(|v| {
                Ok(Change {
                    path: self
                        .path
                        .clone()
                        .try_into()
                        .map_err(|_| Error::AuditConversion)?,
                    kind: "enum_expanded"
                        .try_into()
                        .map_err(|_| Error::AuditConversion)?,
                    from: None,
                    to: Some(v.value.clone()),
                })
            })
            .collect::<Result<Vec<_>, Error>>()?;
        draft.drift = Some(Drift {
            class: DriftClass::Semantic,
            side: match side {
                Side::Request => DriftSide::Request,
                Side::Response => DriftSide::Response,
            },
            changes,
            first_seen: Some(first_seen),
            vendor: None,
            occurrences: None,
        });
        let new_values: Vec<_> = self
            .new_values
            .iter()
            .map(|v| {
                json!({
                    "value": v.value, "count": v.count, "frequency": v.frequency,
                    "first_seen": v.first_seen, "material": v.material,
                })
            })
            .collect();
        draft.metadata.insert(
            "agindow_enum_growth".into(),
            json!({
                "path": self.path,
                "baseline_cardinality": self.cardinalities.baseline,
                "observed_cardinality": self.cardinalities.observed,
                "union_cardinality": self.cardinalities.union,
                "new_values": new_values,
            }),
        );
        draft.hypothesis = None;
        draft.detail = None;
        draft.note = None;
        draft.confidence = None;
        draft.patch = None;
        draft.proof.write_integrity = false;
        Ok(draft)
    }
}
