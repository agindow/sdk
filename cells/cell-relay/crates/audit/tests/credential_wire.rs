use agw_audit::contracts::credential_event::CredentialEvent;
#[test]
fn shared_credential_event_round_trip() -> Result<(), Box<dyn std::error::Error>> {
    let source = include_str!("../../../../../contracts/examples/credential_event.valid.json");
    let event: CredentialEvent = serde_json::from_str(source)?;
    let encoded = serde_json::to_string(&event)?;
    let _: CredentialEvent = serde_json::from_str(&encoded)?;
    assert!(!encoded.contains("sk_live_AGINDOWcanary123"));
    Ok(())
}
#[test]
fn credential_conditionals_are_checked_at_native_boundary() -> Result<(), Box<dyn std::error::Error>>
{
    let source = include_str!("../../../../../contracts/examples/credential_event.valid.json");
    agw_audit::credential_wire::parse(source.as_bytes())?;
    let invalid = include_str!("../../../../../contracts/examples/credential_event.invalid.json");
    assert!(agw_audit::credential_wire::parse(invalid.as_bytes()).is_err());
    let mut value: serde_json::Value = serde_json::from_str(source)?;
    value["secret_ref"] = serde_json::json!("sk_live_AGINDOWcanary123");
    assert!(agw_audit::credential_wire::parse(&serde_json::to_vec(&value)?).is_err());
    Ok(())
}

#[test]
fn secret_shaped_values_are_rejected_in_every_reference_slot()
-> Result<(), Box<dyn std::error::Error>> {
    let source = include_str!("../../../../../contracts/examples/credential_event.valid.json");
    for field in [
        "id",
        "tenant",
        "connection",
        "operation",
        "credential_version",
        "prior_version",
        "prior_event",
        "audit_ref",
    ] {
        let mut value: serde_json::Value = serde_json::from_str(source)?;
        if field == "prior_version" {
            value["cause"] = serde_json::json!("rotated");
            value["evidence"] = serde_json::json!("host_declared");
            value["credential_version"] = serde_json::json!("v2");
            value[field] = serde_json::json!("v1");
        } else if field == "prior_event" {
            value["cause"] = serde_json::json!("recovered");
            value["evidence"] = serde_json::json!("provider_declared");
            value["status"] = serde_json::json!(200);
            value[field] = serde_json::json!("event-0");
        }
        agw_audit::credential_wire::parse(&serde_json::to_vec(&value)?)?;
        value[field] = serde_json::json!("sk_live_AGINDOWcanary123");
        assert!(
            agw_audit::credential_wire::parse(&serde_json::to_vec(&value)?).is_err(),
            "{field}"
        );
    }
    Ok(())
}
