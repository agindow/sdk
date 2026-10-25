use agw_audit::investigation_wire::validate;

const VALID: &[u8] =
    include_bytes!("../../../../../contracts/examples/investigation_job.valid.json");

#[test]
fn validates_the_frozen_payload_free_job_at_the_native_boundary()
-> Result<(), Box<dyn std::error::Error>> {
    validate(VALID)?;
    let input: serde_json::Value = serde_json::from_slice(VALID)?;
    for (field, value) in [
        ("version", serde_json::json!(2)),
        ("tenant", serde_json::json!("")),
        ("tenant", serde_json::json!("wrong/tenant")),
        ("ceiling", serde_json::json!("18446744073709551616")),
        ("ceiling", serde_json::json!("01")),
        ("expires_at", serde_json::json!("2026-10-21T13:00:00Z")),
        ("payload", serde_json::json!({"secret": true})),
    ] {
        let mut changed = input.clone();
        changed[field] = value;
        assert!(validate(&serde_json::to_vec(&changed)?).is_err());
    }
    assert!(validate(&vec![b' '; 4097]).is_err());
    Ok(())
}
