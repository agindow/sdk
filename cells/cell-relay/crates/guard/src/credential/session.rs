//! Post-response adapter: original upstream bytes/status are moved to forward
//! untouched. The host drains the returned fixed-size evidence separately.
use super::{
    Cause, Evidence, Signal,
    health::SetupError,
    tracker::{Observation, Tracker},
};
use crate::proxy::{self, ForwardedResponse, Headers, UpstreamResponse};
use agw_audit::contracts::credential_event::CredentialEvent;
use std::{io::Read, time::Duration};
pub struct Context<'a> {
    pub id: &'a str,
    pub rotation_id: &'a str,
    pub ts: &'a str,
    pub tenant: &'a str,
    pub connection: &'a str,
    pub operation: &'a str,
    pub arguments: &'a str,
    pub version: Option<&'a str>,
    pub audit_ref: Option<&'a str>,
    pub now: u64,
    pub available: bool,
    pub complete: bool,
    pub revoked: bool,
}
pub struct Forwarded<B> {
    pub response: ForwardedResponse<B>,
    pub events: [Option<CredentialEvent>; 2],
}
pub struct Session {
    tracker: Tracker,
    prior_failure: Option<String>,
    window: u64,
    scope: Option<(String, String)>,
}
impl Session {
    pub fn new(operation_cap: usize, threshold: u32, window_ms: u64) -> Result<Self, SetupError> {
        Ok(Self {
            tracker: Tracker::new(operation_cap, threshold, window_ms)?,
            prior_failure: None,
            window: window_ms,
            scope: None,
        })
    }
    pub fn observe(
        &mut self,
        c: &Context<'_>,
        status: u16,
        headers: &Headers,
        body: Option<&[u8]>,
    ) -> [Option<CredentialEvent>; 2] {
        if c.id == c.rotation_id
            || ![
                c.id,
                c.rotation_id,
                c.tenant,
                c.connection,
                c.operation,
                c.arguments,
            ]
            .into_iter()
            .all(super::cutover::reference)
            || c.version.is_some_and(|v| !super::cutover::reference(v))
            || c.audit_ref.is_some_and(|v| !super::cutover::reference(v))
            || !c.available
            || !c.complete
        {
            return [None, None];
        }
        if let Some((tenant, connection)) = &self.scope {
            if tenant != c.tenant || connection != c.connection {
                return [None, None];
            }
        } else {
            self.scope = Some((c.tenant.to_owned(), c.connection.to_owned()));
        }
        let (prior, outcome) = self.tracker.observe_all(&Observation {
            operation: c.operation,
            arguments: c.arguments,
            version: c.version,
            now: c.now,
            signal: Signal {
                status,
                headers,
                body,
                complete: c.complete,
                available: c.available,
                revoked: c.revoked,
            },
        });
        let rotation = prior.as_deref().and_then(|prior| {
            self.event(
                c,
                status,
                Cause::Rotated,
                Evidence::HostDeclared,
                Some(prior),
                None,
            )
        });
        if prior.is_some() {
            self.prior_failure = None;
        }
        let event = outcome.and_then(|(cause, evidence)| {
            self.event(
                c,
                status,
                cause,
                evidence,
                None,
                if cause == Cause::Recovered {
                    self.prior_failure.as_deref()
                } else {
                    None
                },
            )
        });
        if let Some(event) = &event {
            if matches!(outcome, Some((Cause::Recovered, _))) {
                self.prior_failure = None;
            } else if outcome.is_some_and(|(cause, _)| cause.failure()) {
                // The validated event has only a bounded host reference.
                self.prior_failure = serde_json::to_value(event)
                    .ok()
                    .and_then(|v| v["id"].as_str().map(str::to_owned));
            }
        }
        [rotation, event]
    }
    fn event(
        &self,
        c: &Context<'_>,
        status: u16,
        cause: Cause,
        evidence: Evidence,
        prior_version: Option<&str>,
        prior_event: Option<&str>,
    ) -> Option<CredentialEvent> {
        let mut event = serde_json::json!({ "contract":"credential_event", "version":1, "id": c.id, "ts":c.ts, "tenant":c.tenant, "connection":c.connection, "operation":c.operation, "status":status, "cause":cause, "evidence":evidence });
        for (key, value) in [
            ("credential_version", c.version),
            ("audit_ref", c.audit_ref),
            ("prior_version", prior_version),
            ("prior_event", prior_event),
        ] {
            if let Some(value) = value {
                event[key] = serde_json::json!(value);
            }
        }
        if cause == Cause::ScopeReduced {
            event["failure_count"] = serde_json::json!(self.tracker.scope_count());
            event["window_ms"] = serde_json::json!(self.window);
        }
        if cause == Cause::Rotated {
            event["id"] = serde_json::json!(c.rotation_id);
        }
        let bytes = serde_json::to_vec(&event).ok()?;
        agw_audit::credential_wire::parse(&bytes).ok()
    }
    /// Completeness refers to a separately bounded capture; never consumes body.
    pub fn forward<B: Read>(
        &mut self,
        c: &Context<'_>,
        upstream: UpstreamResponse<B>,
        overhead: Duration,
        capture: Option<&[u8]>,
    ) -> Forwarded<B> {
        let events = self.observe(c, upstream.status, &upstream.headers, capture);
        Forwarded {
            response: proxy::forward(upstream, overhead),
            events,
        }
    }
}
