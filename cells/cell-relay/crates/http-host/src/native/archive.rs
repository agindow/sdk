//! Bounded, process-local completed outcomes. No durability/replay claim.
use agw_audit::contracts::audit_artifact::AuditArtifact;
use axum::{
    body::Bytes,
    http::{HeaderMap, StatusCode},
};
use std::collections::VecDeque;
pub struct Entry {
    pub id: String,
    pub artifact: AuditArtifact,
    pub original: Option<(StatusCode, HeaderMap, Bytes)>,
}
#[derive(Default)]
pub struct Archive {
    entries: VecDeque<Entry>,
    bytes: usize,
}
impl Archive {
    pub fn insert(&mut self, entry: Entry) -> bool {
        let size = size(&entry);
        if size > 16 * 1024 * 1024 {
            return false;
        }
        while self.entries.len() >= 128 || self.bytes.saturating_add(size) > 16 * 1024 * 1024 {
            if let Some(old) = self.entries.pop_front() {
                self.bytes = self.bytes.saturating_sub(size_of_entry(&old));
            } else {
                break;
            }
        }
        self.bytes += size;
        self.entries.push_back(entry);
        true
    }
    pub fn get(&self, id: &str) -> Option<&Entry> {
        self.entries.iter().find(|entry| entry.id == id)
    }
}
fn size_of_entry(entry: &Entry) -> usize {
    size(entry)
}
fn size(entry: &Entry) -> usize {
    serde_json::to_vec(&entry.artifact)
        .map_or(usize::MAX, |json| json.len())
        .saturating_add(entry.original.as_ref().map_or(0, |(_, headers, body)| {
            body.len()
                + headers
                    .iter()
                    .map(|(k, v)| k.as_str().len() + v.len())
                    .sum::<usize>()
        }))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn refuses_an_unretainable_completed_outcome() -> Result<(), Box<dyn std::error::Error>> {
        let artifact: AuditArtifact = serde_json::from_value(serde_json::json!({
            "id":"aud_fixture","ts":"2026-10-20T00:00:00Z","tool":"op","operation":"write","mode":"strict",
            "drift":null,"decision":"ok","patch":null,
            "proof":{"contract_version":"unavailable","verified_against":"cache","write_integrity":false}
        }))?;
        let mut archive = Archive::default();
        let retained = archive.insert(Entry {
            id: "aud_fixture".into(),
            artifact,
            original: Some((
                StatusCode::CREATED,
                HeaderMap::new(),
                Bytes::from(vec![0; 16 * 1024 * 1024]),
            )),
        });
        assert!(!retained);
        assert!(archive.get("aud_fixture").is_none());
        Ok(())
    }
}
