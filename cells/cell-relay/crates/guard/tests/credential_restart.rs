use agw_guard::credential::cutover::{Cutover, Definition, Kind, Phase};
#[path = "support/credential_cutover.rs"]
mod fixture;
use fixture::{machine, probe};

#[test]
fn edited_passed_flags_cannot_promote_a_plan_without_its_recorded_probe_evidence()
-> Result<(), Box<dyn std::error::Error>> {
    let mut c = machine()?;
    c.set_plan(vec![
        Definition {
            operation: "read".into(),
            kind: Kind::Read,
            write_opt_in: false,
            idempotency_key_present: false,
        },
        Definition {
            operation: "write".into(),
            kind: Kind::Write,
            write_opt_in: true,
            idempotency_key_present: true,
        },
    ]);
    c.start(0);
    c.record_probe(probe(None), 1);
    assert!(!c.promote(2));
    let mut value: serde_json::Value = serde_json::from_str(&c.snapshot()?)?;
    value[0]["phase"] = serde_json::json!("overlap");
    value[0]["window_start"] = serde_json::json!(2);
    value[0]["window_end"] = serde_json::json!(52);
    value[5] = serde_json::json!([true, true]);
    let restored = Cutover::restore(machine()?, Some(&serde_json::to_string(&value)?));
    assert_eq!(restored.phase(), Phase::Unknown);
    assert_eq!(*restored.dispatch(), "active");
    Ok(())
}

#[test]
fn inconsistent_probe_counts_lifecycle_and_overlap_state_freeze_to_active()
-> Result<(), Box<dyn std::error::Error>> {
    let mut c = machine()?;
    c.start(0);
    c.record_probe(probe(None), 1);
    c.promote(2);
    for case in 0..6 {
        let mut value: serde_json::Value = serde_json::from_str(&c.snapshot()?)?;
        match case {
            0 => value[0]["probes"][0]["status"] = serde_json::json!(401),
            1 => value[0]["probes"][0]["cause"] = serde_json::json!("quota"),
            2 => value[0]["failure_count"] = serde_json::json!(3),
            3 => value[1] = serde_json::Value::Null,
            4 => value[2] = serde_json::json!(53),
            _ => {
                value[0]["phase"] = serde_json::json!("idle");
                value[0]["window_start"] = serde_json::Value::Null;
                value[0]["window_end"] = serde_json::Value::Null;
                value[1] = serde_json::Value::Null;
            }
        }
        let restored = Cutover::restore(machine()?, Some(&serde_json::to_string(&value)?));
        assert_eq!(restored.phase(), Phase::Unknown, "case {case}");
        assert_eq!(*restored.dispatch(), "active", "case {case}");
    }
    Ok(())
}
