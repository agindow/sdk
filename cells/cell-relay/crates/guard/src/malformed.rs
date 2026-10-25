//! TI-36, contract §4.4: complete malformed bodies are never repaired.
//!
//! Explicit library seam, not streaming middleware. The caller publishes the
//! returned artifact and warning, and honors `Halt` before dispatch. Inspect
//! responses after capture; never buffer a stream to make this decision. Other
//! findings belong to the normal diff/policy pipeline (`NotApplicable`).
use std::{num::NonZeroU64, panic::UnwindSafe};

use agw_audit::contracts::audit_artifact::{
    AuditArtifact, AuditArtifactDecision as Decision, AuditArtifactOperation as Operation, Change,
    Drift, DriftClass, DriftSide, Patch,
};
use agw_diff::{Kind, Side, Snapshot, compare_inspection};
use agw_inspector::{Inspection, inspect};
use chrono::{DateTime, Utc};

use crate::{
    availability::{EngineFault, contain},
    config::{CallContext, Mode},
    proxy::Headers,
};

/// Borrowed evidence and explicit tenancy. `scope` identifies the tenant and
/// connection; never share it across tenants. `truncated` is independent of
/// whether the prefix happens to parse. Side is independent of read/write.
#[derive(Clone, Copy)]
pub struct Body<'a> {
    pub bytes: &'a [u8],
    pub truncated: bool,
    pub side: Side,
    pub baseline: &'a Snapshot,
    pub scope: &'a str,
    pub vendor: &'a str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Disposition {
    NotApplicable,
    Bypassed,
    Degraded,
    /// Strict policy escalation, not an internal engine error. No dispatch.
    Halt,
    /// No drift verdict or operational header policy for incomplete captures.
    Incomplete,
}

/// Shared TI-36 disposition; normal policy must not override malformed bodies.
pub fn disposition(mode: Mode, operation: Operation) -> Disposition {
    if mode == Mode::Strict {
        Disposition::Halt
    } else if operation == Operation::Read {
        Disposition::Degraded
    } else {
        Disposition::Bypassed
    }
}

/// Contains original bytes even when dispatch is halted. No repair API exists
/// here, making byte preservation structural. `audit` is absent only when this
/// seam defers to the normal pipeline. Returned artifacts are owned drafts;
/// persistence and async warning delivery remain the caller's responsibility.
#[must_use = "Honor Halt before dispatch and publish the returned audit and warning"]
#[derive(Debug)]
pub struct Outcome<'a> {
    pub body: &'a [u8],
    pub disposition: Disposition,
    pub headers: Headers,
    pub warning: Option<&'static str>,
    pub audit: Option<AuditArtifact>,
}

/// One caller-owned active recurrence scope, constant entry count. Retain one
/// instance per tenant/connection/vendor/tool/operation/side/baseline outside
/// the request path when interleaved scopes must retain independent histories.
/// A changed key resets this slot. Clean, incomplete and faulted inspections do
/// not increment it. No global map, I/O, clock read or synchronization required.
#[derive(Debug, Default)]
pub struct History {
    entry: Option<([String; 6], DateTime<Utc>, NonZeroU64)>,
}

impl History {
    fn record(&mut self, input: Body<'_>, draft: &AuditArtifact) -> (DateTime<Utc>, NonZeroU64) {
        let key = [
            input.scope.into(),
            input.vendor.into(),
            draft.tool.to_string(),
            draft.operation.to_string(),
            format!("{:?}", input.side),
            draft.proof.contract_version.to_string(),
        ];
        if let Some((prior, first, count)) = &mut self.entry
            && *prior == key
        {
            *count = count.saturating_add(1);
            return (*first, *count);
        }
        self.entry = Some((key, draft.ts, NonZeroU64::MIN));
        (draft.ts, NonZeroU64::MIN)
    }
}

impl CallContext {
    pub fn malformed<'a>(
        &self,
        input: Body<'a>,
        history: &mut History,
        draft: AuditArtifact,
    ) -> Outcome<'a> {
        self.malformed_with(input, history, draft, |bytes, truncated| {
            inspect(bytes, truncated, None)
        })
    }

    /// Injectable parser seam for native fault containment. Resource/Data/I/O
    /// failures bypass; only syntax/EOF on a complete capture is vendor drift.
    /// The parser must describe `bytes`, never substitute another document.
    pub fn malformed_with<'a, F>(
        &self,
        input: Body<'a>,
        history: &mut History,
        draft: AuditArtifact,
        parser: F,
    ) -> Outcome<'a>
    where
        F: FnOnce(&[u8], bool) -> Inspection + UnwindSafe,
    {
        let mut outcome = Outcome {
            body: input.bytes,
            disposition: Disposition::NotApplicable,
            headers: Headers::new(),
            warning: None,
            audit: None,
        };
        let report = contain(move || {
            if input.truncated {
                // Still dispose of the supplied closure inside containment.
                drop(parser);
                return Ok(None);
            }
            let inspection = parser(input.bytes, input.truncated);
            if inspection.truncated {
                return Ok(None);
            }
            Ok(Some(compare_inspection(
                input.side,
                input.baseline,
                &inspection,
            )))
        });
        let mut draft = self.audit(draft);
        let detail = match report {
            Ok(None) => {
                outcome.disposition = Disposition::Incomplete;
                "inspection_incomplete"
            }
            Ok(Some(report))
                if report.skipped.is_none()
                    && report.changes.len() == 1
                    && report.changes[0].kind == Kind::MalformedBody =>
            {
                // Build fallible contract fields before updating recurrence.
                match malformed_drift(input) {
                    Ok(mut drift) => {
                        let (first, count) = history.record(input, &draft);
                        drift.first_seen = Some(first);
                        drift.occurrences = Some(count);
                        draft.drift = Some(drift);
                        draft.confidence = Some(0.0);
                        outcome.disposition = disposition(self.mode(), draft.operation);
                        outcome.headers.claim("X-Agindow-Drift", "detected");
                        outcome.headers.claim("X-Agindow-Drift-Class", "shape");
                        outcome.warning = Some(if self.mode() == Mode::Strict {
                            "drift_halted: malformed body; human escalation required; repair skipped"
                        } else {
                            "malformed_body: original bytes passed through; repair skipped"
                        });
                        "malformed_body"
                    }
                    Err(_) => {
                        outcome.disposition = Disposition::Bypassed;
                        "inspection_fault"
                    }
                }
            }
            Ok(Some(report)) if report.skipped.is_none() => return outcome,
            Ok(Some(_)) | Err(_) => {
                outcome.disposition = Disposition::Bypassed;
                "inspection_fault"
            }
        };
        if detail != "malformed_body" {
            draft.drift = None;
            draft.confidence = None;
        }
        // An input draft may contain prior claims; never inherit a repair or a
        // successful integrity proof for bytes we could not validate.
        draft.hypothesis = None;
        draft.note = None;
        draft.detail = detail.try_into().ok();
        draft.proof.write_integrity = false;
        draft.patch = Some(Patch {
            applied: false,
            byte_identical: true,
            fields_changed: Vec::new(),
            selective: None,
            reasoning: None,
        });
        draft.decision = match outcome.disposition {
            Disposition::Halt => Decision::Blocked,
            Disposition::Degraded => Decision::Degraded,
            _ => Decision::Bypassed,
        };
        if outcome.disposition != Disposition::Incomplete {
            outcome
                .headers
                .claim("X-Agindow-Mode", self.mode().to_string());
            outcome
                .headers
                .claim("X-Agindow-Status", draft.decision.to_string());
            outcome.headers.claim("X-Agindow-Repair", "skipped");
        }
        outcome.audit = Some(draft);
        outcome
    }
}

fn malformed_drift(input: Body<'_>) -> Result<Drift, EngineFault> {
    Ok(Drift {
        class: DriftClass::Shape,
        side: match input.side {
            Side::Request => DriftSide::Request,
            Side::Response => DriftSide::Response,
        },
        vendor: Some(input.vendor.try_into().map_err(|_| EngineFault::Internal)?),
        changes: vec![Change {
            path: "$".try_into().map_err(|_| EngineFault::Internal)?,
            kind: "malformed_body"
                .try_into()
                .map_err(|_| EngineFault::Internal)?,
            from: None,
            to: None,
        }],
        first_seen: None,
        occurrences: None,
    })
}
