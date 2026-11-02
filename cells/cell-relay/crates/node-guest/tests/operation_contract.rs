use agw_node_guest::Guest;
use serde_json::{Value, json};
fn input(side: &str, kind: &str, before: &str, after: &str) -> Value {
    let scope = json!({"route":"/charges","method":"POST","side":side,"version":"v1"});
    json!({"baseline":{"scope":scope,"kind":kind,"provenance":"review-a","body":before,"complete":true},
        "current":{"scope":scope,"kind":kind,"provenance":"review-b","body":after,"complete":true}})
}
fn compare(input: Value) -> Result<Value, String> {
    let guest = Guest::new("null", 0.0)?;
    serde_json::from_str(&guest.compare_operation(&input.to_string(), 200_000_000)?)
        .map_err(|e| e.to_string())
}
#[test]
fn directional_declared_nested_constraints() -> Result<(), String> {
    let a = r#"{"type":"object","properties":{"nested":{"type":"object","properties":{"x":{"type":"string","enum":["a"]}}}}}"#;
    let b = r#"{"type":"object","properties":{"nested":{"type":"object","properties":{"x":{"type":["string","null"],"enum":["a","b"]},"added":{"type":"string"}}}}}"#;
    let request = compare(input("request", "declared", a, b))?;
    assert_eq!(request["coverage"], "complete");
    assert!(
        request["findings"]
            .as_array()
            .ok_or("findings")?
            .iter()
            .all(|v| v["compatibility"] == "compatible")
    );
    let response = compare(input("response", "declared", a, b))?;
    assert!(
        response["findings"]
            .as_array()
            .ok_or("findings")?
            .iter()
            .any(|v| v["compatibility"] == "breaking")
    );
    Ok(())
}
#[test]
fn observations_never_declare_requiredness_and_scope_isolation() -> Result<(), String> {
    let result = compare(input(
        "response",
        "observed",
        r#"{"nested":{"x":1}}"#,
        r#"{"nested":{}}"#,
    ))?;
    assert_eq!(result["findings"][0]["compatibility"], "unknown");
    assert_eq!(result["findings"][0]["path"], "nested.x");
    assert_eq!(result["requiredness"], "unknown");
    let mut mismatch = input("request", "observed", "{}", "{}");
    mismatch["current"]["scope"]["route"] = json!("/other");
    assert_eq!(compare(mismatch)?["coverage"], "unsupported");
    Ok(())
}
#[test]
fn gaps_never_fabricate_findings() -> Result<(), String> {
    for (kind, a, b) in [
        ("observed", r#"{"items":[]}"#, r#"{"items":[1,"x"]}"#),
        ("observed", r#"{"x":1,"x":2}"#, r#"{"x":1}"#),
        (
            "declared",
            r#"{"type":"string","oneOf":[]}"#,
            r#"{"type":"number"}"#,
        ),
    ] {
        let result = compare(input("response", kind, a, b))?;
        assert_ne!(result["coverage"], "complete");
        assert_eq!(result["findings"], json!([]));
    }
    let mut incomplete = input("request", "observed", "{}", "{}");
    incomplete["current"]["complete"] = json!(false);
    assert_eq!(compare(incomplete)?["findings"], json!([]));
    Ok(())
}

#[test]
fn shared_operation_corpus() -> Result<(), String> {
    let rows: Vec<Value> = serde_json::from_str(include_str!(
        "../../../node/parity/operation-contracts.json"
    ))
    .map_err(|e| e.to_string())?;
    for row in rows {
        let result = compare(row["input"].clone())?;
        for field in ["coverage", "requiredness"] {
            assert_eq!(result[field], row["expected"][field], "{}", row["id"]);
        }
        let mut actual = result["findings"]
            .as_array()
            .ok_or("findings")?
            .iter()
            .map(|v| json!({"path":v["path"],"compatibility":v["compatibility"]}))
            .collect::<Vec<_>>();
        let mut expected = row["expected"]["findings"]
            .as_array()
            .ok_or("expected")?
            .clone();
        actual.sort_by_key(|v| v["path"].to_string());
        expected.sort_by_key(|v| v["path"].to_string());
        assert_eq!(actual, expected, "{}", row["id"]);
    }
    Ok(())
}
