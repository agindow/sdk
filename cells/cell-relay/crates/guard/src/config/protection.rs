use super::{ConfigError, OPT_OUT, RuntimeConfig};
use chrono::{DateTime, Utc};
use std::sync::atomic::Ordering;

pub const WRITE_PROTECTION_WARNING: &str = "AGINDOW: fail-open writes requested. A semantically drifted write may return 200 and persist wrong data. Semantic writes remain protected in repair mode; strict mode ignores this opt-out.";

/// Administrative control audit, separate from a call's frozen audit schema.
/// The host binds this receipt to its tenant/connection and persists it off-path.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ProtectionReceipt {
    pub fail_closed: bool,
    pub actor: String,
    pub ts: DateTime<Utc>,
    pub warning: Option<&'static str>,
}

impl RuntimeConfig {
    /// Administrative request, never an in-path operation. Require attribution
    /// even for restoration. Invalid metadata leaves configuration untouched.
    /// Keep one RuntimeConfig per protection scope; retain returned receipts.
    pub fn reload_fail_closed(
        &self,
        fail_closed: bool,
        actor: &str,
        ts: DateTime<Utc>,
    ) -> Result<ProtectionReceipt, ConfigError> {
        if actor.trim().is_empty() || actor.len() > 256 || actor.chars().any(char::is_control) {
            return Err(ConfigError {
                source: "reload.actor",
                requirement: "expected 1 through 256 bytes without control characters",
            });
        }
        let receipt = ProtectionReceipt {
            fail_closed,
            actor: actor.into(),
            ts,
            warning: (!fail_closed).then_some(WRITE_PROTECTION_WARNING),
        };
        if fail_closed {
            self.mode.fetch_and(!OPT_OUT, Ordering::Relaxed);
        } else {
            self.mode.fetch_or(OPT_OUT, Ordering::Relaxed);
        }
        Ok(receipt)
    }
}
