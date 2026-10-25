use agw_guard::credential::{Cause, Evidence, cutover::*};
use std::sync::Arc;
pub fn machine() -> Result<Cutover<&'static str>, Box<dyn std::error::Error>> {
    let mut machine = Cutover::new(
        "cutover-1",
        Arc::new("active"),
        Arc::new("candidate"),
        Config {
            dual_key: Some(true),
            max_probes: 4,
            deadline: 100,
            overlap_ms: 50,
            threshold: 3,
        },
    )?;
    assert!(machine.set_plan(vec![Definition {
        operation: "read".into(),
        kind: Kind::Read,
        write_opt_in: false,
        idempotency_key_present: false
    }]));
    Ok(machine)
}
pub fn probe(cause: Option<Cause>) -> Probe {
    Probe {
        operation: "read".into(),
        kind: Kind::Read,
        write_opt_in: false,
        idempotency_key_present: false,
        status: if cause.is_some() { 401 } else { 200 },
        latency_ms: 1,
        audit_ref: "audit-1".into(),
        cause,
        evidence: Evidence::Unknown,
    }
}
