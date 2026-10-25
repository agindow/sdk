use agw_node_guest::Guest;
use serde_json::Value;

#[test]
fn byte_payloads_preserve_utf8_parsing_and_two_phase_verdicts() -> Result<(), String> {
    let selection = serde_json::json!({ "tenant": "tenant", "connection": "c", "vendor": "v",
        "operation": "op", "segment": "s", "materiality": 0.2,
        "fields": [{ "side": "request", "path": "/value", "baseline": vec!["{\"value\":1}";128],
            "live": vec!["{\"value\":100}";128], "enumeration": true },
            { "side": "response", "path": "/value", "baseline": vec!["{\"value\":1}";128],
            "live": vec!["{\"value\":1}";128], "enumeration": true }] })
    .to_string();
    let guest = Guest::new(&selection, 0.0)?;
    let call = guest.begin(
        "{\"value\":\"é😀\"}".as_bytes(),
        false,
        "observe",
        "read",
        1.0,
        true,
        200_000_000,
    )?;
    let request: Value = serde_json::from_str(&call.request().map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    assert_eq!(request["detectors"], 5);
    assert_eq!(request["bypassed"], false);
    let response: Value = serde_json::from_str(
        &call
            .finish("{\"value\":\"é😀\"}".as_bytes(), false, 2.0)
            .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    assert_eq!(response["detectors"], 5);
    assert_eq!(response["bypassed"], false);
    Ok(())
}
