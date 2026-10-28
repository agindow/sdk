use agw_node_guest::Guest;
use serde_json::{Value, json};
#[test]
fn prepares_arbitrary_reviewed_renames_without_rewriting_other_bytes() -> Result<(), String> {
    let guest = Guest::new("null", 0.0)?;
    let input = br#" { "old_key" : {"value":7}, "unchanged":1e400 } "#;
    let result: Value = serde_json::from_str(&guest.prepare_rename(
        input,
        "old_key",
        "new_key",
        1.0,
        "repair",
        200_000_000,
    )?)
    .map_err(|e| e.to_string())?;
    assert_eq!(result["applied"], true);
    assert_eq!(
        result["body"],
        " { \"new_key\" : {\"value\":7}, \"unchanged\":1e400 } "
    );
    for (mode, score) in [("observe", 1.0), ("repair", 0.96), ("strict", 0.98)] {
        let value: Value = serde_json::from_str(&guest.prepare_rename(
            input,
            "old_key",
            "new_key",
            score,
            mode,
            200_000_000,
        )?)
        .map_err(|e| e.to_string())?;
        assert_eq!(value["applied"], false);
    }
    let collision: Value = serde_json::from_str(&guest.prepare_rename(
        br#"{"old_key":1,"new_key":2}"#,
        "old_key",
        "new_key",
        1.0,
        "repair",
        200_000_000,
    )?)
    .map_err(|e| e.to_string())?;
    assert_eq!(collision["applied"], false);
    let absent: Value = serde_json::from_str(&guest.prepare_rename(
        br#"{"other":1}"#,
        "old_key",
        "new_key",
        1.0,
        "repair",
        200_000_000,
    )?)
    .map_err(|e| e.to_string())?;
    assert_eq!(absent, json!({"matched":false,"applied":false}));
    Ok(())
}
