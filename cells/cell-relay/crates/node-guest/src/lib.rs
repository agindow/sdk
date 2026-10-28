//! TI-30 production binding. Host values and upstream execution stay in Node.
mod audit;
mod playground;
mod selection;
mod selective;
use agw_guard::{
    config::{Config, RuntimeConfig},
    statistical::{Body, Engine, Operation, Phase, Session},
};
use chrono::DateTime;
use serde_json::json;
use std::{cell::RefCell, rc::Rc};
use wasm_bindgen::prelude::*;

/// Setup-only local selection adapter shared by explicit HTTP hosts.
pub fn local_engine(selection: &str) -> Result<Engine, String> {
    selection::engine(selection)
}

fn tick(now: f64) -> Result<(), String> {
    if !now.is_finite() || now < 0.0 {
        return Err("invalid monotonic time".into());
    }
    #[cfg(target_arch = "wasm32")]
    agw_guard::readiness::advance_host_time(std::time::Duration::from_secs_f64(now / 1000.0));
    Ok(())
}
fn verdict(phase: &Phase) -> String {
    json!({ "halt": phase.decision == agw_guard::statistical::Decision::Halt,
        "bypassed": phase.fault.is_some(),
        "class": phase.report.class().map(|c| c.to_string()),
        "findings": phase.report.changes.len(), "detectors": phase.records.len(),
        "malformed": phase.malformed.is_some(),
        "detectorKinds": phase.records.iter().map(|record|format!("{:?}", record.detector)).collect::<Vec<_>>(),
        "eligibleDetectors": phase.records.iter().filter(|record|record.eligibility.iter().flatten().any(|d|d.eligible()))
            .map(|record|format!("{:?}",record.detector)).collect::<Vec<_>>(),
        "suppressedDetectors": phase.records.iter().filter(|record|record.eligibility.iter().flatten().any(|d|!d.eligible()))
            .map(|record|format!("{:?}",record.detector)).collect::<Vec<_>>(),
        "fault": phase.fault.map(|fault| format!("{fault:?}")),
    })
    .to_string()
}
fn body(bytes: &[u8], truncated: bool) -> Body<'_> {
    Body {
        bytes,
        truncated,
        at: DateTime::UNIX_EPOCH,
        findings: None,
    }
}
#[wasm_bindgen]
pub struct Guest {
    engine: Engine,
    active: bool,
    tenant: Option<String>,
    audit: audit::Shared,
}
#[wasm_bindgen]
impl Guest {
    #[wasm_bindgen(constructor)]
    pub fn new(selection: &str, now: f64) -> Result<Guest, String> {
        tick(now)?;
        Ok(Self {
            engine: selection::engine(selection)?,
            active: selection != "null",
            tenant: if selection == "null" {
                None
            } else {
                Some(
                    serde_json::from_str::<selection::Input>(selection)
                        .map_err(|e| e.to_string())?
                        .tenant,
                )
            },
            audit: Rc::new(RefCell::new(audit::Outbox::default())),
        })
    }
    pub fn enable_audit(
        &self,
        tenant: &str,
        namespace: &str,
        capacity: u32,
        max_bytes: u32,
    ) -> Result<(), String> {
        if self
            .tenant
            .as_deref()
            .is_some_and(|selected| selected != tenant)
        {
            return Err("audit tenant must match selected scope".into());
        }
        self.audit
            .try_borrow_mut()
            .map_err(|_| "audit busy")?
            .enable(tenant, namespace, capacity, max_bytes)
    }
    pub fn audit_peek(&self) -> String {
        self.audit
            .try_borrow()
            .map_or_else(|_| "null".into(), |queue| queue.peek())
    }
    pub fn audit_ack(&self, id: &str) -> bool {
        self.audit
            .try_borrow_mut()
            .is_ok_and(|mut queue| queue.ack(id))
    }
    pub fn audit_stats(&self) -> String {
        self.audit
            .try_borrow()
            .map_or_else(|_| "null".into(), |queue| queue.stats())
    }
    /// Fixture-scoped verified repair. No vendor I/O; not a default SDK pipeline.
    pub fn playground_rename(&self, bytes: &[u8], mode: &str, work: u32) -> Result<String, String> {
        playground::run(bytes, mode, work)
    }
    /// Prepare a generic reviewed rename. The SDK must independently verify it.
    pub fn prepare_rename(
        &self,
        bytes: &[u8],
        source: &str,
        target: &str,
        confidence: f64,
        mode: &str,
        work: u32,
    ) -> Result<String, String> {
        selective::prepare(bytes, source, target, confidence, mode, work)
    }
    pub fn set_ready(&self, ready: bool) {
        self.engine.set_ready(ready);
    }
    /// Whether selected evidence needs payload capture, not engine identity.
    /// False still uses the constructed statistical engine and readiness gate.
    pub fn active(&self) -> bool {
        self.active
    }
    /// Fused core execution for hosts with expensive FFI crossings. Only the
    /// immutable empty statistical plan may finish before upstream: it inspects neither
    /// side and has no call-local evidence. Selected engines must suspend.
    pub fn null_call(
        &self,
        mode: u32,
        operation: u32,
        fail_closed: bool,
        work: u32,
        now: f64,
    ) -> Result<bool, String> {
        if self.active || self.audit.try_borrow().is_ok_and(|queue| queue.enabled) {
            return Err("selected or audited engines require two phases".into());
        }
        let mode = match mode {
            0 => "observe",
            1 => "repair",
            2 => "strict",
            _ => return Err("invalid mode".into()),
        };
        let operation = match operation {
            0 => "read",
            1 => "write",
            2 => "unknown",
            _ => return Err("invalid operation".into()),
        };
        let mut call = self.begin(b"{}", false, mode, operation, now, fail_closed, work)?;
        let session = call.session.take().ok_or("call finished")?;
        let request_clean = clean(session.request());
        let outcome = session.finish(body(b"{}", false));
        Ok(request_clean && outcome.response.as_ref().is_some_and(clean))
    }
    /// Start a suspended call from the host's captured JSON bytes. Bytes are
    /// parsed exactly as the former string boundary did; invalid UTF-8 remains
    /// an unparseable body, never a host error.
    #[allow(clippy::too_many_arguments)]
    pub fn begin(
        &self,
        request: &[u8],
        truncated: bool,
        mode: &str,
        operation: &str,
        now: f64,
        fail_closed: bool,
        work: u32,
    ) -> Result<GuestCall, String> {
        tick(now)?;
        let config = RuntimeConfig::from_sources(
            Config {
                mode: Some(mode.into()),
                ..Config::default()
            },
            None,
        )
        .map_err(|e| e.to_string())?;
        config
            .reload_fail_closed(fail_closed, "node-sdk", DateTime::UNIX_EPOCH)
            .map_err(|e| e.to_string())?;
        let operation = match operation {
            "read" => Operation::Read,
            "write" => Operation::Write,
            "unknown" => Operation::Unknown,
            _ => return Err("invalid operation".into()),
        };
        let session = self
            .engine
            .begin_call(
                config.begin_call(None).map_err(|e| e.to_string())?,
                operation,
                "sdk",
                work as usize,
                0,
            )
            .start(body(request, truncated));
        Ok(GuestCall {
            session: Some(session),
            audit: Rc::clone(&self.audit),
            context: None,
        })
    }
}
fn clean(phase: &Phase) -> bool {
    phase.decision != agw_guard::statistical::Decision::Halt
        && phase.fault.is_none()
        && phase.report.changes.is_empty()
        && phase.records.is_empty()
        && phase.malformed.is_none()
}
#[wasm_bindgen]
pub struct GuestCall {
    session: Option<Session<'static>>,
    audit: audit::Shared,
    context: Option<audit::Context>,
}
#[wasm_bindgen]
impl GuestCall {
    pub fn audit_context(&mut self, tool: &str, ts: &str) -> Result<(), String> {
        if tool.is_empty()
            || tool.len() > 256
            || tool.chars().any(char::is_control)
            || self.context.is_some()
        {
            return Err("invalid audit context".into());
        }
        self.context = Some(audit::Context {
            tool: tool.into(),
            ts: DateTime::parse_from_rfc3339(ts)
                .map_err(|e| e.to_string())?
                .to_utc(),
        });
        Ok(())
    }
    pub fn request(&self) -> Result<String, String> {
        self.session
            .as_ref()
            .map(|s| verdict(s.request()))
            .ok_or("call finished".into())
    }
    /// Finish with the host's completed-response JSON bytes. A truncated
    /// capture keeps the unavailable-projection path; complete bytes parse.
    pub fn finish(mut self, response: &[u8], truncated: bool, now: f64) -> Result<String, String> {
        tick(now)?;
        let session = self.session.take().ok_or("call finished")?;
        let captured = body(response, truncated);
        let outcome = if truncated {
            session.finish_unavailable(captured)
        } else {
            session.finish(captured)
        };
        if let Ok(mut queue) = self.audit.try_borrow_mut() {
            queue.publish(&outcome, self.context.take());
        }
        Ok(outcome
            .response
            .as_ref()
            .map(verdict)
            .unwrap_or_else(|| verdict(&outcome.request)))
    }
}
