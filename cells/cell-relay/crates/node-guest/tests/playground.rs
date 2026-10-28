use agw_node_guest::Guest;
use serde_json::Value;

#[test]
fn reviewed_rename_is_verified_selective_and_never_the_default() -> Result<(), String> {
    let guest = Guest::new("null", 0.0)?;
    let source = br#"{ "contact_name" : "Ada", "untouched": 1.00, "memo": "keep me" }"#;
    let result: Value =
        serde_json::from_str(&guest.playground_rename(source, "repair", 200_000_000)?)
            .map_err(|e| e.to_string())?;
    assert_eq!(result["repairApplied"], true);
    assert_eq!(result["verified"], true);
    assert_eq!(
        result["after"],
        r#"{ "display_name" : "Ada", "untouched": 1.00, "memo": "keep me" }"#
    );
    let observed: Value =
        serde_json::from_str(&guest.playground_rename(source, "observe", 200_000_000)?)
            .map_err(|e| e.to_string())?;
    assert_eq!(observed["repairApplied"], false);
    assert_eq!(
        observed["after"],
        std::str::from_utf8(source).map_err(|e| e.to_string())?
    );
    for bad in [
        br#"{"source":"Ada"}"#.as_slice(),
        br#"{"contact_name":"Ada","display_name":"other"}"#,
    ] {
        let result: Value =
            serde_json::from_str(&guest.playground_rename(bad, "repair", 200_000_000)?)
                .map_err(|e| e.to_string())?;
        assert_eq!(result["repairApplied"], false);
    }
    let fault: Value = serde_json::from_str(&guest.playground_rename(source, "repair", 0)?)
        .map_err(|e| e.to_string())?;
    assert_eq!(fault["bypassed"], true);
    assert_eq!(fault["repairApplied"], false);
    Ok(())
}
