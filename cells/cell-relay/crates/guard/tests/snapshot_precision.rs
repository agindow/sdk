//! TI-38: learned metadata -> v2 snapshot -> field-local lossless inspection.
use agw_audit::contracts::contract_snapshot::ContractSnapshot;
use agw_guard::snapshot::{precision_for, with_learned_precision};
use agw_inspector::inference::{BaselineStore, Key, Limits};
use agw_inspector::{Number, Parse, Value, inspect_fields};
use std::{error::Error, num::NonZeroUsize};
type Result = std::result::Result<(), Box<dyn Error>>;

#[test]
fn validated_snapshot_drives_precision_without_touching_unflagged_numbers() -> Result {
    let snapshot: ContractSnapshot = serde_json::from_str(include_str!(
        "../../../../../contracts/examples/contract_snapshot.valid.json"
    ))?;
    let tool = snapshot.tools.first().ok_or("tool")?;
    let fields = precision_for(tool)?;
    let bytes = br#"{"id":18446744073709551616,"other":18446744073709551616}"#;
    let Parse::Parsed(Value::Object(values)) = inspect_fields(bytes, false, &fields).parse else {
        return Err("object".into());
    };
    assert!(matches!(
        values.first(),
        Some((_, Value::Number(Number::Verbatim(_))))
    ));
    assert!(matches!(
        values.get(1),
        Some((_, Value::Number(Number::Unrepresentable(_))))
    ));
    assert_eq!(serde_json::to_vec(&Value::Object(values))?, bytes);
    Ok(())
}

#[test]
fn learned_flags_annotate_existing_snapshot_fields_and_preserve_explicit_choices() -> Result {
    let mut store = BaselineStore::new(NonZeroUsize::new(2).ok_or("count")?, Limits::default());
    let key = Key::new("stripe", "call");
    store.begin(key.clone(), None)?;
    for _ in 0..2 {
        store.observe(&key, br#"{"id":18446744073709551616}"#, false)?;
    }
    let schema = &store.latest(&key).ok_or("baseline")?.schema;
    let mut snapshot: ContractSnapshot = serde_json::from_str(include_str!(
        "../../../../../contracts/examples/contract_snapshot.valid.json"
    ))?;
    let tool = snapshot.tools.first_mut().ok_or("tool")?;
    let field = tool.fields.first_mut().ok_or("field")?;
    field.high_precision = None;
    let annotated = with_learned_precision(tool, schema);
    assert_eq!(
        annotated.fields.first().ok_or("field")?.high_precision,
        Some(true)
    );
    assert_eq!(tool.fields.first().ok_or("field")?.high_precision, None);
    let map = precision_for(&annotated)?;
    assert_eq!(map.at("id"), agw_inspector::Precision::HighPrecision);
    tool.fields.first_mut().ok_or("field")?.high_precision = Some(false);
    assert_eq!(
        with_learned_precision(tool, schema)
            .fields
            .first()
            .ok_or("field")?
            .high_precision,
        Some(false)
    );
    Ok(())
}
