use agw_node_guest::Guest;
use serde_json::{Value, json};

fn facts(recipe: &str) -> Value {
    json!({"version":1,"recipe":recipe,"tool":"pay","side":"request","contract":"v2",
        "migration":"review-1","provenance":"owner-review","expiresAt":4102444800000u64,"pointer":"/value"})
}
fn prepare(body: &str, facts: Value, confidence: f64, mode: &str) -> Result<Value, String> {
    let guest = Guest::new("null", 0.0)?;
    serde_json::from_str(&guest.prepare_scalar(
        body.as_bytes(),
        &facts.to_string(),
        confidence,
        mode,
        200_000_000,
    )?)
    .map_err(|e| e.to_string())
}
#[test]
fn string_mapping_and_empty_null_preserve_every_other_byte() -> Result<(), String> {
    let mut rule = facts("string-map");
    rule["mapping"] = json!([["old", "new"]]);
    let body = " {\"value\":\"old\", \"id\":9007199254740993,\"huge\":1e400} ";
    assert_eq!(
        prepare(body, rule.clone(), 1.0, "repair")?["body"],
        body.replace("\"old\"", "\"new\"")
    );
    assert_eq!(prepare(body, rule.clone(), 0.989, "repair")?["halt"], true);
    assert_eq!(
        prepare(body, rule.clone(), 1.0, "observe")?["applied"],
        false
    );
    for body in [r#"{"value":"unknown"}"#, r#"{"value":4}"#, "{}"] {
        let result = prepare(body, rule.clone(), 1.0, "repair")?;
        assert_eq!(result["applied"], false);
        assert_eq!(result["halt"], true);
    }
    let mut null = facts("empty-null");
    null["nullMeaning"] = json!("empty means absent");
    assert_eq!(
        prepare(r#"{"value":""}"#, null.clone(), 1.0, "repair")?["body"],
        r#"{"value":null}"#
    );
    for body in [
        r#"{"value":" "}"#,
        r#"{"value":null}"#,
        r#"{"value":0}"#,
        "{}",
    ] {
        assert_eq!(
            prepare(body, null.clone(), 1.0, "repair")?["applied"],
            false
        );
    }
    rule["mapping"] = json!([["a", "new"], ["b", "new"]]);
    assert!(prepare(r#"{"value":"a"}"#, rule.clone(), 1.0, "repair").is_err());
    rule["equivalence"] = json!([["a", "review-a"], ["b", "review-b"]]);
    assert_eq!(
        prepare(r#"{"value":"a"}"#, rule, 1.0, "repair")?["applied"],
        true
    );
    Ok(())
}
fn scale() -> Value {
    let mut rule = facts("scale");
    rule["sourceUnit"] = json!("cents");
    rule["targetUnit"] = json!("dollars");
    rule["numerator"] = json!("1");
    rule["denominator"] = json!("100");
    rule["inputMin"] = json!("-999999999999999999");
    rule["inputMax"] = json!("999999999999999999");
    rule["outputMin"] = json!("-999999999999999999");
    rule["outputMax"] = json!("999999999999999999");
    rule
}
#[test]
fn exact_numeric_scaling_and_refusal_boundaries() -> Result<(), String> {
    for (source, target) in [
        ("9007199254740993", "90071992547409.93"),
        ("-125", "-1.25"),
        ("0", "0"),
        ("1.25e2", "1.25"),
    ] {
        assert_eq!(
            prepare(
                &format!("{{\"value\":{source},\"untouched\":1e400}}"),
                scale(),
                1.0,
                "repair"
            )?["body"],
            format!("{{\"value\":{target},\"untouched\":1e400}}")
        );
    }
    for source in [
        "1e400",
        "0.0000000000000000001",
        "\"125\"",
        "1000000000000000000",
    ] {
        assert_eq!(
            prepare(&format!("{{\"value\":{source}}}"), scale(), 1.0, "repair")?["applied"],
            false
        );
    }
    let mut bad = scale();
    bad["denominator"] = json!("3");
    assert!(prepare(r#"{"value":1}"#, bad, 1.0, "repair").is_err());
    assert!(prepare(r#"{"value":1,"value":2}"#, scale(), 1.0, "repair").is_err());
    Ok(())
}

#[test]
fn shared_scalar_corpus() -> Result<(), String> {
    let rows: Vec<Value> =
        serde_json::from_str(include_str!("../../../node/parity/scalar-repairs.json"))
            .map_err(|e| e.to_string())?;
    for row in rows {
        let body = row["before"].as_str().ok_or("before")?;
        let result = prepare(
            body,
            row["facts"].clone(),
            row["confidence"].as_f64().ok_or("confidence")?,
            "repair",
        );
        if let Some(after) = row["after"].as_str() {
            assert_eq!(result?["body"], after, "{}", row["id"]);
        } else if let Ok(result) = result {
            assert_eq!(result["applied"], false, "{}", row["id"]);
        }
    }
    Ok(())
}
