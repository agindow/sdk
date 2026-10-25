//! Local configuration and per-call snapshots — TI-09, contract §10.6.
//!
//! Read environment only at startup. A control-plane owner publishes updates
//! through [`RuntimeConfig::reload_mode`]; requests only load a local atomic.
//! This module selects and records modes, not drift or policy decisions.

mod protection;
pub use protection::{ProtectionReceipt, WRITE_PROTECTION_WARNING};

use std::error::Error;
use std::fmt;
use std::io::Read;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Duration;

use agw_audit::contracts::audit_artifact::AuditArtifact;
use agw_inspector::tee::{DEFAULT_CAP, Tee};

use crate::proxy::{self, ForwardedResponse, UpstreamResponse};

/// Reuse the frozen contract enum so configuration and audit cannot diverge.
pub use agw_audit::contracts::audit_artifact::AuditArtifactMode as Mode;

const MODE_MASK: u8 = 3;
const OPT_OUT: u8 = 4;

pub const X_AGINDOW_MODE: &str = "X-Agindow-Mode";

/// Startup configuration. All supplied values are validated, even if shadowed.
#[derive(Debug, Clone, Default)]
pub struct Config {
    /// Explicit mode wins over `AGINDOW_MODE`; absent both, use `repair`.
    pub mode: Option<String>,
    /// Startup-only capture limit: 0 disables capture, maximum is 8 MiB.
    pub inspection_cap: Option<usize>,
}

/// Invalid input is reported at the configuration/request boundary, before
/// execution. It is not an engine failure that should silently pass through.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError {
    source: &'static str,
    requirement: &'static str,
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid {}: {}", self.source, self.requirement)
    }
}

impl Error for ConfigError {}

fn parse_mode(value: &str, source: &'static str) -> Result<Mode, ConfigError> {
    value.parse().map_err(|_| ConfigError {
        source,
        requirement: "expected exactly observe, repair, or strict",
    })
}

// Encode explicitly rather than depending on the generated enum's layout.
fn encode(mode: Mode) -> u8 {
    match mode {
        Mode::Observe => 0,
        Mode::Repair => 1,
        Mode::Strict => 2,
    }
}

/// Shared local mode control. Clones publish to the same atomic; no locks,
/// network calls, environment reads or filesystem access occur at call entry.
#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    mode: Arc<AtomicU8>,
    inspection_cap: usize,
}

impl RuntimeConfig {
    /// Resolve the process environment once, outside the request path.
    pub fn from_env(config: Config) -> Result<Self, ConfigError> {
        let env = match std::env::var("AGINDOW_MODE") {
            Ok(value) => Some(value),
            Err(std::env::VarError::NotPresent) => None,
            Err(std::env::VarError::NotUnicode(_)) => {
                return Err(ConfigError {
                    source: "AGINDOW_MODE",
                    requirement: "expected a Unicode mode value",
                });
            }
        };
        Self::from_sources(config, env.as_deref())
    }

    /// Resolve an environment snapshot: config > env > repair.
    /// Both supplied modes must be valid, even if one is shadowed.
    pub fn from_sources(config: Config, env_mode: Option<&str>) -> Result<Self, ConfigError> {
        let explicit = config
            .mode
            .as_deref()
            .map(|v| parse_mode(v, "config.mode"))
            .transpose()?;
        let env = env_mode
            .map(|v| parse_mode(v, "AGINDOW_MODE"))
            .transpose()?;
        let mode = explicit.or(env).unwrap_or(Mode::Repair);
        let inspection_cap = config.inspection_cap.unwrap_or(DEFAULT_CAP);
        if inspection_cap > DEFAULT_CAP {
            return Err(ConfigError {
                source: "config.inspection_cap",
                requirement: "expected 0 through 8388608 bytes",
            });
        }
        Ok(Self {
            mode: Arc::new(AtomicU8::new(encode(mode))),
            inspection_cap,
        })
    }

    /// Publish a mode for subsequent calls. Parse before storing so rejected
    /// updates cannot change the current policy. Existing calls retain theirs.
    pub fn reload_mode(&self, value: &str) -> Result<(), ConfigError> {
        let mode = parse_mode(value, "reload.mode")?;
        let mut state = self.mode.load(Ordering::Relaxed);
        loop {
            match self.mode.compare_exchange_weak(
                state,
                (state & !MODE_MASK) | encode(mode),
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(current) => state = current,
            }
        }
        Ok(())
    }

    /// Snapshot once before execution. A request override is local to this call
    /// and never writes shared configuration.
    pub fn begin_call(&self, override_mode: Option<&str>) -> Result<CallContext, ConfigError> {
        let state = self.mode.load(Ordering::Relaxed);
        let mode = match override_mode {
            Some(value) => parse_mode(value, "request.mode")?,
            None => match state & MODE_MASK {
                0 => Mode::Observe,
                1 => Mode::Repair,
                // Only encode() writes this private atomic; 2 means strict.
                _ => Mode::Strict,
            },
        };
        Ok(CallContext {
            mode,
            inspection_cap: self.inspection_cap,
            fail_closed: state & OPT_OUT == 0,
        })
    }
}

/// The effective configuration for one call, retained through all phases.
/// It has no link back to the live mode, so reload cannot relabel its proof.
#[derive(Debug, Clone, Copy)]
pub struct CallContext {
    mode: Mode,
    inspection_cap: usize,
    fail_closed: bool,
}

impl CallContext {
    /// TI-26: frozen alongside mode; never consult live controls during a call.
    pub fn fail_closed(&self) -> bool {
        self.fail_closed
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn inspection_cap(&self) -> usize {
        self.inspection_cap
    }

    /// Forward without re-encoding, using this call's cap and mode header.
    /// This does not implement repair or enforce policy for any mode yet.
    pub fn forward<B: Read>(
        &self,
        upstream: UpstreamResponse<B>,
        overhead: Duration,
    ) -> ForwardedResponse<Tee<B>> {
        let UpstreamResponse {
            status,
            headers,
            body,
        } = upstream;
        let upstream = UpstreamResponse {
            status,
            headers,
            body: Tee::with_cap(body, self.inspection_cap),
        };
        let mut response = proxy::forward(upstream, overhead);
        response
            .headers
            .claim(X_AGINDOW_MODE, self.mode.to_string());
        response
    }

    /// Finalize the mode on an owned draft for every outcome, including faults
    /// and blocked calls. All other proof fields remain the producer's job.
    /// Never reads live configuration or mutates an already-published artifact.
    pub fn audit(&self, mut draft: AuditArtifact) -> AuditArtifact {
        draft.mode = self.mode;
        draft
    }
}
