//! Shared TII-28 test helpers. Never a test file itself.
#![allow(dead_code)]

use agw_audit::contracts::notification::{
    ConsolidationMode, FindingClass, FindingSide, Notification, NotificationSeverity,
};
use agw_notify::{
    emitter::{Sink, SinkError},
    envelope::{self, Alert},
};
use chrono::{DateTime, Utc};

pub const REFS: &[&str] = &["aud_review_1"];

/** Parse one fixture time; tests own the failure message. */
pub fn at(value: &str) -> DateTime<Utc> {
    match value.parse() {
        Ok(time) => time,
        Err(error) => panic!("invalid fixture time {value}: {error}"),
    }
}

/** One individual alert, identical to the shared valid fixture's content. */
pub fn base() -> Alert<'static> {
    Alert {
        tenant: "acme",
        connection: "conn_1",
        vendor: "stripe",
        operation: "create_customer",
        class: FindingClass::Shape,
        side: FindingSide::Request,
        severity: NotificationSeverity::Warning,
        occurrences: 3,
        first_seen: at("2026-10-21T09:00:00Z"),
        last_seen: at("2026-10-21T11:59:00Z"),
        audit_refs: REFS,
        day: 20_747,
        ordinal: 2,
        mode: ConsolidationMode::Individual,
        consolidated: 1,
        ts: at("2026-10-21T12:00:00Z"),
    }
}

#[derive(Default)]
pub struct MemorySink {
    pub tenant: Option<&'static str>,
    pub ids: Vec<String>,
}

impl Sink for MemorySink {
    fn tenant(&self) -> Option<&str> {
        self.tenant.or(Some("acme"))
    }
    fn emit(&mut self, notification: &Notification) -> std::result::Result<(), SinkError> {
        self.ids.push(notification.id.as_str().to_string());
        Ok(())
    }
}

pub fn built(alert: Alert<'_>) -> Notification {
    match envelope::build(alert) {
        Ok(value) => value,
        Err(error) => panic!("invalid fixture alert: {error}"),
    }
}
