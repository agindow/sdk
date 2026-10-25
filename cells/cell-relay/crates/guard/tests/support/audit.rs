//! Shared helpers for composed statistical audit tests (TI-47).
#![allow(dead_code)]
use agw_audit::contracts::audit_artifact::AuditArtifact;
use agw_audit::sink::{Sink, SinkError};
use std::error::Error;

pub fn draft() -> Result<AuditArtifact, Box<dyn Error>> {
    Ok(serde_json::from_str(include_str!(
        "../../../../../../contracts/examples/audit_artifact.valid.json"
    ))?)
}

pub fn statistics(artifact: &AuditArtifact) -> Result<&serde_json::Value, Box<dyn Error>> {
    artifact
        .metadata
        .get("agindow_statistical")
        .ok_or_else(|| "missing agindow_statistical metadata".into())
}

pub fn findings(artifact: &AuditArtifact) -> Result<&Vec<serde_json::Value>, Box<dyn Error>> {
    statistics(artifact)?["findings"]
        .as_array()
        .ok_or_else(|| "findings array".into())
}

pub fn finding<'a>(
    artifact: &'a AuditArtifact,
    detector: &str,
    path: &str,
) -> Result<&'a serde_json::Value, Box<dyn Error>> {
    findings(artifact)?
        .iter()
        .find(|finding| finding["detector"] == detector && finding["path"] == path)
        .ok_or_else(|| format!("missing {detector} finding for {path}").into())
}

pub struct Offline;

impl Sink for Offline {
    fn emit(&mut self, _artifact: &AuditArtifact) -> Result<(), SinkError> {
        Err(SinkError::new("offline"))
    }
}
