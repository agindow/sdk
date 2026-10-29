//! TII-150 structural fixture parity only; scope/time/digest admission is TS-owned.
use agw_audit::contracts::source_repair_evidence::SourceRepairEvidence;

const VALID: &str =
    include_str!("../../../../../contracts/examples/source_repair_evidence.valid.json");
const INVALID: &str =
    include_str!("../../../../../contracts/examples/source_repair_evidence.invalid.json");

#[test]
fn shared_examples() -> Result<(), Box<dyn std::error::Error>> {
    let _: SourceRepairEvidence = serde_json::from_str(VALID)?;
    assert!(serde_json::from_str::<SourceRepairEvidence>(INVALID).is_err());
    Ok(())
}

#[test]
fn checked_references() -> Result<(), Box<dyn std::error::Error>> {
    for (path, text) in [
        ("/scope/tenant", "contains spaces"),
        ("/finding/field", "/nested/field"),
        ("/digest", "not-a-digest"),
    ] {
        let mut value: serde_json::Value = serde_json::from_str(VALID)?;
        let slot = value.pointer_mut(path).ok_or("missing fixture field")?;
        *slot = serde_json::Value::String(text.into());
        assert!(serde_json::from_value::<SourceRepairEvidence>(value).is_err());
    }
    Ok(())
}
