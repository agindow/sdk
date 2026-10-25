//! TI-18: the generated Rust boundary consumes the shared profile fixtures.
use agw_audit::contracts::field_profile::FieldProfile;

#[test]
fn profile_fixtures_match_the_typescript_boundary() -> Result<(), serde_json::Error> {
    let profile: FieldProfile = serde_json::from_str(include_str!(
        "../../../../../contracts/examples/field_profile.valid.json"
    ))?;
    assert_eq!(profile.window_id.as_str(), "w1");
    assert!(
        serde_json::from_str::<FieldProfile>(include_str!(
            "../../../../../contracts/examples/field_profile.invalid.json"
        ))
        .is_err()
    );
    Ok(())
}

#[test]
fn profile_arrays_and_pointers_keep_schema_constraints() -> Result<(), serde_json::Error> {
    let valid: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../contracts/examples/field_profile.valid.json"
    ))?;
    for field in ["hll", "values"] {
        let mut invalid = valid.clone();
        invalid[field] = serde_json::json!([0, 0]);
        assert!(serde_json::from_value::<FieldProfile>(invalid).is_err());
    }
    for identifier in [String::new(), "x".repeat(257)] {
        let mut invalid = valid.clone();
        invalid["scope"]["tenant"] = serde_json::json!(identifier);
        assert!(serde_json::from_value::<FieldProfile>(invalid).is_err());
    }
    for pointer in ["field", "/bad~2", "/trailing~"] {
        let mut invalid = valid.clone();
        invalid["scope"]["field"] = serde_json::json!(pointer);
        assert!(serde_json::from_value::<FieldProfile>(invalid).is_err());
    }
    for pointer in ["", "/a~0b/~1"] {
        let mut accepted = valid.clone();
        accepted["scope"]["field"] = serde_json::json!(pointer);
        let _: FieldProfile = serde_json::from_value(accepted)?;
    }
    Ok(())
}
