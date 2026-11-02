use agw_node_guest::Guest;
use serde_json::{Value, json};

fn prepare(body: &str, parents: &[&str], mode: &str) -> Result<Value, String> {
    let guest = Guest::new("null", 0.0)?;
    let selection = json!({"source":"old.key", "parents":parents}).to_string();
    serde_json::from_str(&guest.prepare_nested_rename(
        body.as_bytes(),
        &selection,
        "new/key",
        1.0,
        mode,
        200_000_000,
    )?)
    .map_err(|e| e.to_string())
}

#[test]
fn selects_only_concrete_elements_and_preserves_raw_bytes() -> Result<(), String> {
    let body = r#" {"items":[{"old.key":9007199254740993},{"old.key":1e400},{"old.key":-0}],"sibling":"old.key"} "#;
    let result = prepare(body, &["/items/0", "/items/2"], "repair")?;
    assert_eq!(result["applied"], true);
    assert_eq!(
        result["body"],
        body.replacen("\"old.key\":900", "\"new/key\":900", 1)
            .replace("\"old.key\":-0", "\"new/key\":-0")
    );
    assert_eq!(
        result["changes"],
        json!([
            {"path":"items[0].old\\.key", "to":"items[0].new/key"},
            {"path":"items[2].old\\.key", "to":"items[2].new/key"}
        ])
    );
    Ok(())
}

#[test]
fn refuses_atomic_collisions_missing_targets_duplicates_and_overlaps() -> Result<(), String> {
    for body in [
        r#"{"items":[{"old.key":1},{"old.key":2,"new/key":3}]}"#,
        r#"{"items":[{"old.key":1},{}]}"#,
    ] {
        assert_eq!(
            prepare(body, &["/items/0", "/items/1"], "repair")?["applied"],
            false
        );
    }
    assert!(prepare(r#"{"old.key":1,"old.key":2}"#, &[""], "repair").is_err());
    assert!(prepare(r#"{"old.key":{"old.key":1}}"#, &["", "/old.key"], "repair").is_err());
    assert!(prepare(r#"{"old.key":1}"#, &["", ""], "repair").is_err());
    Ok(())
}

#[test]
fn escaped_pointers_observe_and_work_bounds() -> Result<(), String> {
    let body = r#"{"a/b~c":{"old.key":"kept"}}"#;
    assert_eq!(prepare(body, &["/a~1b~0c"], "strict")?["applied"], true);
    assert_eq!(prepare(body, &["/a~1b~0c"], "observe")?["applied"], false);
    assert!(prepare(body, &vec!["/x"; 17], "repair").is_err());
    assert!(prepare("{", &[""], "repair").is_err());
    let guest = Guest::new("null", 0.0)?;
    assert!(
        guest
            .prepare_nested_rename(
                body.as_bytes(),
                r#"{"source":"old.key","parents":["/a~1b~0c"]}"#,
                "new/key",
                1.0,
                "repair",
                1
            )
            .is_err()
    );
    Ok(())
}

#[derive(serde::Deserialize)]
struct Fixture {
    id: String,
    source: String,
    target: String,
    parents: Vec<String>,
    before: String,
    after: Option<String>,
}
#[test]
fn shared_sdk_corpus() -> Result<(), String> {
    let fixtures: Vec<Fixture> =
        serde_json::from_str(include_str!("../../../node/parity/nested-renames.json"))
            .map_err(|e| e.to_string())?;
    let guest = Guest::new("null", 0.0)?;
    for fixture in fixtures {
        let result = guest.prepare_nested_rename(
            fixture.before.as_bytes(),
            &json!({"source":fixture.source,"parents":fixture.parents}).to_string(),
            &fixture.target,
            1.0,
            "repair",
            200_000_000,
        );
        match (result, fixture.after) {
            (Ok(result), expected) => {
                let result: Value = serde_json::from_str(&result).map_err(|e| e.to_string())?;
                assert_eq!(result["applied"], expected.is_some(), "{}", fixture.id);
                if let Some(after) = expected {
                    assert_eq!(result["body"], after, "{}", fixture.id);
                }
            }
            (Err(_), None) => {}
            (Err(error), Some(_)) => return Err(format!("{}: {error}", fixture.id)),
        }
    }
    Ok(())
}
