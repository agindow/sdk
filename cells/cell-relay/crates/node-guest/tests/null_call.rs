use agw_node_guest::Guest;

#[test]
fn fused_null_call_uses_the_core_for_every_valid_mode_and_operation() -> Result<(), String> {
    let guest = Guest::new("null", 0.0)?;
    for mode in 0..3 {
        for operation in 0..3 {
            assert!(guest.null_call(mode, operation, true, 0, 1.0)?);
        }
    }
    assert!(guest.null_call(3, 0, true, 0, 1.0).is_err());
    assert!(guest.null_call(0, 3, true, 0, 1.0).is_err());
    let selected = Guest::new(
        r#"{"tenant":"t","connection":"c","vendor":"v","operation":"op","segment":"s","fields":[]}"#,
        1.0,
    )?;
    assert!(selected.null_call(0, 0, true, 0, 1.0).is_err());
    Ok(())
}
