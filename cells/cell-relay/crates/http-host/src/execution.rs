//! A two-phase shared-core suspension; no transport or credentials enter it.
use crate::Envelope;
use agw_audit::contracts::audit_artifact::AuditArtifact;
use agw_guard::{
    config::{Config, RuntimeConfig},
    feedback::{FeedbackCode, Payload},
    statistical::{Body, Decision, Engine, Outcome, Session},
};
use chrono::{DateTime, Utc};
use serde_json::json;

pub struct Execution {
    pub session: Session<'static>,
    tool: String,
    id: String,
    at: DateTime<Utc>,
    machine_authority: Option<serde_json::Value>,
}
impl Execution {
    pub fn start(
        input: &Envelope,
        engine: &Engine,
        id: String,
        work: usize,
    ) -> Result<Self, String> {
        Self::start_at(input, engine, id, work, Utc::now())
    }
    pub fn start_at(
        input: &Envelope,
        engine: &Engine,
        id: String,
        work: usize,
        at: DateTime<Utc>,
    ) -> Result<Self, String> {
        let config = RuntimeConfig::from_sources(
            Config {
                mode: Some(input.mode.to_string()),
                ..Config::default()
            },
            None,
        )
        .map_err(|error| error.to_string())?;
        let session = engine
            .begin_call(
                config.begin_call(None).map_err(|e| e.to_string())?,
                input.operation,
                "http",
                work,
                0,
            )
            .start(Body::complete(input.args.get().as_bytes(), at));
        Ok(Self {
            session,
            tool: input.tool.clone(),
            id,
            at,
            machine_authority: None,
        })
    }
    /// Accept only verified claims from the authorization boundary, never
    /// metadata supplied by the caller. No token or proof bytes enter audit.
    pub fn with_machine_authority(
        mut self,
        claims: &crate::machine::Claims,
    ) -> Result<Self, String> {
        self.machine_authority = Some(
            json!({"jti":claims.jti,"tenant":claims.tenant,"environment":claims.environment,
            "aud":claims.aud,"connection":claims.connection,"tools":claims.tools,"exp":claims.exp,"act":claims.act}),
        );
        Ok(self)
    }
    pub fn request_halted(&self) -> bool {
        self.session.request().decision == Decision::Halt
    }
    pub fn finish(self, bytes: &[u8], truncated: bool) -> Result<Completed, String> {
        let captured = Body {
            bytes,
            truncated,
            at: self.at,
            findings: None,
        };
        let outcome = if truncated {
            self.session.finish_unavailable(captured)
        } else {
            self.session.finish(captured)
        };
        let draft = serde_json::from_value(json!({ "id": self.id, "ts": self.at, "tool": self.tool,
            "operation": "read", "mode": "observe", "drift": null, "decision": "ok", "patch": null,
            "proof": { "contract_version": "unavailable", "verified_against": "cache", "write_integrity": false },
            "metadata": self.machine_authority.map_or_else(||json!({}),|authority| json!({"machine_auth":authority}))
        })).map_err(|e| e.to_string())?;
        let artifact = outcome.compose(draft);
        let halted = outcome.request.decision == Decision::Halt
            || outcome
                .response
                .as_ref()
                .is_some_and(|phase| phase.decision == Decision::Halt);
        let feedback = policy_feedback(&outcome, &artifact);
        Ok(Completed {
            halted,
            bypassed: outcome.bypasses > 0 || (truncated && outcome.dispatched),
            artifact,
            feedback,
            dispatched: outcome.dispatched,
        })
    }
}
pub struct Completed {
    pub halted: bool,
    pub bypassed: bool,
    pub dispatched: bool,
    pub artifact: AuditArtifact,
    pub feedback: Option<Payload>,
}
fn policy_feedback(outcome: &Outcome<()>, artifact: &AuditArtifact) -> Option<Payload> {
    if outcome.bypasses > 0 || outcome.mode == agw_guard::config::Mode::Observe {
        return None;
    }
    let drift = artifact.drift.as_ref()?;
    let phase = [&outcome.request]
        .into_iter()
        .chain(outcome.response.as_ref())
        .filter(|phase| !phase.report.changes.is_empty())
        .max_by_key(|phase| phase.decision)?;
    let error = match phase.decision {
        Decision::Halt if outcome.mode == agw_guard::config::Mode::Strict => {
            FeedbackCode::DriftHalted
        }
        Decision::Halt => FeedbackCode::DriftBlocked,
        Decision::Degraded => FeedbackCode::DriftPassthrough,
        _ => return None,
    };
    Some(Payload {
        error,
        tool: artifact.tool.to_string(),
        drift_class: drift.class,
        drift_id: artifact.id.to_string(),
        audit: format!("/v1/audit/{}", artifact.id.as_str()),
        hint: if outcome.dispatched {
            "Upstream completed. Inspect the retained outcome; do not blindly replay a write."
        } else {
            "Request was not dispatched. Review the local drift evidence before retrying."
        }
        .into(),
    })
}
