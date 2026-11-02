//! Closed supplied-schema projection. Unsupported constraints never disappear silently.
use agw_diff::Node;
use agw_inspector::inference::JsonType as T;
use serde_json::Value;
use std::collections::BTreeSet;
fn kind(s: &str) -> Option<T> {
    Some(match s {
        "null" => T::Null,
        "boolean" => T::Boolean,
        "integer" => T::Integer,
        "number" => T::Number,
        "string" => T::String,
        "object" => T::Object,
        "array" => T::Array,
        _ => return None,
    })
}
pub(crate) fn node(value: &Value, depth: usize, nodes: &mut usize) -> Result<Node, String> {
    *nodes += 1;
    if depth > 32 || *nodes > 4096 {
        return Err("schema-limit".into());
    }
    let object = value.as_object().ok_or("unsupported-schema")?;
    if object.keys().any(|k| {
        !matches!(
            k.as_str(),
            "type" | "properties" | "required" | "enum" | "items"
        )
    }) {
        return Err("unsupported-keyword".into());
    }
    let types = match object.get("type") {
        Some(Value::String(s)) => BTreeSet::from([kind(s).ok_or("unsupported-type")?]),
        Some(Value::Array(types)) if !types.is_empty() && types.len() <= 7 => {
            let mut set = BTreeSet::new();
            for t in types {
                if !set
                    .insert(kind(t.as_str().ok_or("unsupported-type")?).ok_or("unsupported-type")?)
                {
                    return Err("duplicate-type".into());
                }
            }
            set
        }
        _ => return Err("missing-declared-type".into()),
    };
    let mut result = Node::scalar(T::Null);
    result.types = types;
    if let Some(properties) = object.get("properties") {
        if !result.types.contains(&T::Object) {
            return Err("invalid-properties".into());
        }
        for (key, value) in properties.as_object().ok_or("invalid-properties")? {
            result
                .properties
                .insert(key.clone(), node(value, depth + 1, nodes)?);
        }
    }
    if let Some(required) = object.get("required") {
        for key in required.as_array().ok_or("invalid-required")? {
            let key = key.as_str().ok_or("invalid-required")?;
            if !result.properties.contains_key(key) || !result.required.insert(key.into()) {
                return Err("invalid-required".into());
            }
        }
    }
    if let Some(items) = object.get("items") {
        if !result.types.contains(&T::Array) {
            return Err("invalid-items".into());
        }
        result.items = Some(Box::new(node(items, depth + 1, nodes)?));
    }
    if let Some(values) = object.get("enum") {
        let values = values.as_array().ok_or("unsupported-enum")?;
        if values.is_empty()
            || values.len() > 64
            || !result
                .types
                .iter()
                .all(|t| matches!(t, T::String | T::Null))
        {
            return Err("unsupported-enum".into());
        }
        let mut set = BTreeSet::new();
        let mut admitted = BTreeSet::new();
        for v in values {
            let t = match v {
                Value::String(_) => T::String,
                Value::Null => T::Null,
                _ => return Err("unsupported-enum".into()),
            };
            if !result.types.contains(&t) {
                return Err("invalid-enum-type".into());
            }
            admitted.insert(t);
            if !set.insert(serde_json::to_string(v).map_err(|_| "invalid-enum")?) {
                return Err("duplicate-enum".into());
            }
        }
        result.types = admitted;
        result.enum_values = Some(set);
    }
    Ok(result)
}
