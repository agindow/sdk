//! TI-10: standalone learning, not drift classification or repair confidence.
use std::collections::BTreeSet;

use agw_inspector::inference::{JsonType, Key, LearnError, Progress, SpecPrior};
use serde_json::json;

#[path = "support/learning.rs"]
mod learning;
use learning::{key, store};

#[test]
fn learns_nested_json_schema_without_a_spec() -> Result<(), LearnError> {
    let mut store = store(2);
    let key = key();
    assert_eq!(store.begin(key.clone(), None)?, 1);
    assert_eq!(
        store.observe(&key, br#"{"data":{"user":{"email":"a"}}}"#, false)?,
        Progress::Warming {
            samples: 1,
            required: 2
        }
    );
    assert!(store.latest(&key).is_none());
    assert_eq!(
        store.observe(&key, br#"{"data":{"user":{"email":null}}}"#, false)?,
        Progress::Established { version: 1 }
    );
    let Some(baseline) = store.latest(&key) else {
        panic!("missing baseline")
    };
    let nodes = baseline.schema.paths();
    assert_eq!(
        nodes["data.user.email"].types,
        BTreeSet::from([JsonType::Null, JsonType::String])
    );
    assert_eq!(nodes["data.user.email"].confidence, 1.0);
    assert_eq!(nodes["$"].observations, 2);
    assert_eq!(baseline.samples, 2);
    assert_eq!(
        baseline.schema.json_schema(),
        json!({
            "type": ["object"], "properties": {"data": {
                "type": ["object"], "properties": {"user": {
                    "type": ["object"], "properties": {"email": {"type": ["null", "string"]}}
                }}
            }}
        })
    );
    Ok(())
}

#[test]
fn pools_array_items_but_weights_presence_once_per_response() -> Result<(), LearnError> {
    let mut store = store(3);
    let key = key();
    store.begin(key.clone(), None)?;
    for body in [
        br#"{"rows":[{"id":1},{"id":2},null,"x",[true]]}"#.as_slice(),
        br#"{"rows":[]}"#,
        br#"{}"#,
    ] {
        store.observe(&key, body, false)?;
    }
    let Some(b) = store.latest(&key) else {
        panic!("missing")
    };
    let paths = b.schema.paths();
    assert_eq!(paths["rows"].confidence, 2.0 / 3.0);
    assert_eq!(paths["rows[]"].observations, 1);
    assert_eq!(paths["rows[].id"].confidence, 1.0 / 3.0);
    assert_eq!(paths["rows[][]"].types, BTreeSet::from([JsonType::Boolean]));
    assert_eq!(
        paths["rows[]"].types,
        BTreeSet::from([
            JsonType::Array,
            JsonType::Null,
            JsonType::Object,
            JsonType::String
        ])
    );
    assert_eq!(
        b.schema.json_schema()["properties"]["rows"]["items"]["properties"]["id"],
        json!({"type": ["integer"]})
    );
    Ok(())
}

#[test]
fn versions_freeze_and_keys_are_isolated() -> Result<(), LearnError> {
    let mut store = store(1);
    let a = key();
    let b = Key::new("other", "GET /users");
    let c = Key::new("vendor", "POST /users");
    for k in [&a, &b, &c] {
        store.begin(k.clone(), None)?;
        store.observe(k, b"true", false)?;
    }
    let original = store.latest(&a).cloned();
    assert_eq!(
        store.observe(&a, b"null", false)?,
        Progress::Frozen { version: 1 }
    );
    assert_eq!(store.latest(&a).cloned(), original);
    assert_eq!(store.begin(a.clone(), None)?, 2);
    assert_eq!(
        store.latest(&a).cloned(),
        original,
        "old baseline remains during learning"
    );
    store.observe(&a, b"[]", false)?;
    assert_eq!(store.version(&a, 1).cloned(), original);
    assert_eq!(store.latest(&a).map(|v| v.version), Some(2));
    assert_eq!(store.latest(&b).map(|v| v.version), Some(1));
    assert_eq!(store.latest(&c).map(|v| v.version), Some(1));
    assert_eq!(store.begin(a, None)?, 3);
    Ok(())
}

#[test]
fn prior_is_one_pseudo_sample_not_a_constraint_or_a_shortcut() -> Result<(), LearnError> {
    let prior = SpecPrior::object([
        ("id".into(), SpecPrior::scalar(JsonType::String)),
        ("missing".into(), SpecPrior::scalar(JsonType::Boolean)),
    ]);
    let mut store = store(2);
    let key = key();
    store.begin(key.clone(), Some(prior))?;
    assert_eq!(
        store.observe(&key, br#"{"id":7,"live":null}"#, false)?,
        Progress::Warming {
            samples: 1,
            required: 2
        }
    );
    assert!(store.latest(&key).is_none());
    store.observe(&key, br#"{"id":8,"live":false}"#, false)?;
    let Some(b) = store.latest(&key) else {
        panic!("missing")
    };
    let paths = b.schema.paths();
    assert_eq!(
        paths["id"].types,
        BTreeSet::from([JsonType::String, JsonType::Integer])
    );
    assert_eq!(paths["id"].confidence, 1.0);
    assert_eq!(paths["missing"].confidence, 1.0 / 3.0);
    assert_eq!(paths["live"].confidence, 2.0 / 3.0);
    Ok(())
}

#[test]
fn preserves_numeric_shape_without_float_parsing() -> Result<(), LearnError> {
    let mut store = store(1);
    let key = key();
    store.begin(key.clone(), None)?;
    store.observe(
        &key,
        br#"[1844674407370955161512345,1e400,-0,0.10000000000000000001]"#,
        false,
    )?;
    let Some(b) = store.latest(&key) else {
        panic!("missing")
    };
    assert_eq!(
        b.schema.paths()["$[]"].types,
        BTreeSet::from([JsonType::Integer, JsonType::Number])
    );
    Ok(())
}

#[test]
fn escapes_keys_without_colliding_with_nested_or_array_paths() -> Result<(), LearnError> {
    let mut store = store(1);
    let key = key();
    store.begin(key.clone(), None)?;
    store.observe(
        &key,
        br#"{"a.b":1,"a":{"b":true},"a[]":null,"$":"s","\\":[],"":0}"#,
        false,
    )?;
    let Some(b) = store.latest(&key) else {
        panic!("missing")
    };
    let paths = b.schema.paths();
    assert!(paths.contains_key(r"a\.b"));
    assert!(paths.contains_key("a.b"));
    assert!(paths.contains_key(r"a\[\]"));
    assert!(paths.contains_key(r"\$"));
    assert!(paths.contains_key(r"\\"));
    assert!(paths.contains_key(""));
    assert!(paths.contains_key("$"));
    Ok(())
}

#[test]
fn empty_array_has_no_invented_item_schema() -> Result<(), LearnError> {
    let mut store = store(1);
    let key = key();
    store.begin(key.clone(), None)?;
    store.observe(&key, b"[]", false)?;
    let Some(b) = store.latest(&key) else {
        panic!("missing")
    };
    assert_eq!(b.schema.json_schema(), json!({"type": ["array"]}));
    assert_eq!(b.schema.paths().len(), 1);
    Ok(())
}

#[test]
fn a_failed_restart_preserves_samples_and_prior_array_evidence() -> Result<(), LearnError> {
    let mut store = store(2);
    let key = key();
    store.begin(
        key.clone(),
        Some(SpecPrior::array(SpecPrior::scalar(JsonType::String))),
    )?;
    store.observe(&key, b"[1,2,3]", false)?;
    assert_eq!(
        store.begin(key.clone(), None),
        Err(LearnError::AlreadyLearning)
    );
    assert_eq!(
        store.observe(&key, b"[]", false)?,
        Progress::Established { version: 1 }
    );
    let Some(b) = store.latest(&key) else {
        panic!("missing")
    };
    let paths = b.schema.paths();
    assert_eq!(paths["$[]"].observations, 2);
    assert_eq!(paths["$[]"].confidence, 2.0 / 3.0);
    assert_eq!(
        paths["$[]"].types,
        BTreeSet::from([JsonType::Integer, JsonType::String])
    );
    // Already frozen: no parsing, capture validation or mutation is necessary.
    assert_eq!(
        store.observe(&key, b"invalid", true)?,
        Progress::Frozen { version: 1 }
    );
    Ok(())
}
