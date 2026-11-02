//! Explicit local operation comparisons. No baseline store or policy authority.
use agw_diff::{Evidence, Limits, Node, Side, Snapshot};
use agw_inspector::{Parse, Value};
use serde::Deserialize;
use serde_json::json;
use std::collections::BTreeMap;
#[derive(Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct Scope {
    route: String,
    method: String,
    side: String,
    version: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Source {
    scope: Scope,
    kind: String,
    provenance: String,
    body: String,
    complete: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    baseline: Source,
    current: Source,
}
fn gaps(value: &Value) -> bool {
    match value {
        Value::Array(items) => {
            items.is_empty()
                || items
                    .windows(2)
                    .any(|p| shape_kind(&p[0]) != shape_kind(&p[1]))
                || items.iter().any(gaps)
        }
        Value::Object(entries) => entries.iter().any(|(_, value)| gaps(value)),
        _ => false,
    }
}
fn shape_kind(value: &Value) -> String {
    match value {
        Value::Null => "null".into(),
        Value::Bool(_) => "boolean".into(),
        Value::String(_) => "string".into(),
        Value::Number(_) => "number".into(),
        Value::Array(_) => "array".into(),
        Value::Object(entries) => format!(
            "object:{:?}",
            entries
                .iter()
                .map(|(k, v)| (k, shape_kind(v)))
                .collect::<BTreeMap<_, _>>()
        ),
    }
}
fn schema_gaps(node: &Node) -> bool {
    (node
        .types
        .contains(&agw_inspector::inference::JsonType::Array)
        && node.items.is_none())
        || node.items.as_deref().is_some_and(schema_gaps)
        || node.properties.values().any(schema_gaps)
}
fn snapshot(source: &Source) -> Result<(Snapshot, bool), String> {
    let inspection = agw_inspector::inspect(source.body.as_bytes(), false, None);
    let Parse::Parsed(value) = &inspection.parse else {
        return Err("parse-unavailable".into());
    };
    crate::nested::validate(value, 0, &mut 0).map_err(|_| "duplicate-or-resource-limit")?;
    if source.kind == "declared" {
        let declared = serde_json::from_str(&source.body).map_err(|_| "invalid-schema")?;
        let node = crate::declarations::node(&declared, 0, &mut 0)?;
        let gap = schema_gaps(&node);
        Ok((
            Snapshot {
                node,
                evidence: Evidence::Authoritative,
            },
            gap,
        ))
    } else if source.kind == "observed" {
        let node = agw_diff::observed_shape(&inspection, Limits::default())
            .map_err(|_| "observed-shape-limit")?;
        Ok((
            Snapshot {
                node,
                evidence: Evidence::Learned,
            },
            gaps(value),
        ))
    } else {
        Err("unsupported-source".into())
    }
}
fn concrete(value: &Value, schema: &str, path: &str, map: &mut BTreeMap<String, Vec<String>>) {
    map.entry(schema.into()).or_default().push(path.into());
    match value {
        Value::Object(entries) => {
            for (key, value) in entries {
                let key = crate::nested::escape(key);
                let next = if schema == "$" {
                    key.clone()
                } else {
                    format!("{schema}.{key}")
                };
                let concrete_path = if path == "$" {
                    key
                } else {
                    format!("{path}.{key}")
                };
                concrete(value, &next, &concrete_path, map);
            }
        }
        Value::Array(items) => {
            for (index, value) in items.iter().enumerate() {
                concrete(
                    value,
                    &format!("{schema}[]"),
                    &format!("{path}[{index}]"),
                    map,
                );
            }
        }
        _ => {}
    }
}
pub(crate) fn compare(input: &str, work: u32) -> Result<String, String> {
    let unavailable = |coverage: &str, reason: &str| {
        json!({"coverage":coverage,"gaps":[reason],"requiredness":"unknown","findings":[]})
            .to_string()
    };
    if input.len() > 2_200_000 || input.len().saturating_mul(16) > work as usize {
        return Ok(unavailable("partial", "resource-limit"));
    }
    let input: Input = serde_json::from_str(input).map_err(|_| "invalid operation comparison")?;
    let a = &input.baseline;
    let b = &input.current;
    if a.scope != b.scope
        || !matches!(a.scope.side.as_str(), "request" | "response")
        || !matches!(
            a.scope.method.as_str(),
            "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD" | "OPTIONS"
        )
        || !a.scope.route.starts_with('/')
        || a.scope.route.len() > 256
        || a.scope.version.is_empty()
        || a.scope.version.len() > 256
        || [&a.provenance, &b.provenance]
            .into_iter()
            .any(|s| s.is_empty() || s.len() > 256)
        || a.kind != b.kind
    {
        return Ok(unavailable("unsupported", "scope-or-provenance-mismatch"));
    }
    if !a.complete || !b.complete {
        return Ok(unavailable("partial", "incomplete-capture"));
    }
    if a.body.len() > 1_048_576 || b.body.len() > 1_048_576 {
        return Ok(unavailable("partial", "capture-limit"));
    }
    let (a_node, a_gap) = match snapshot(a) {
        Ok(n) => n,
        Err(reason) => return Ok(unavailable("unsupported", &reason)),
    };
    let (mut b_node, b_gap) = match snapshot(b) {
        Ok(n) => n,
        Err(reason) => return Ok(unavailable("unsupported", &reason)),
    };
    // Missing array evidence cannot establish a whole-operation conclusion.
    if a_gap || b_gap {
        return Ok(unavailable("partial", "unseen-or-heterogeneous-items"));
    }
    let declared = a.kind == "declared";
    if !declared {
        b_node.evidence = Evidence::Observed;
    }
    let side = if a.scope.side == "request" {
        Side::Request
    } else {
        Side::Response
    };
    let report = agw_diff::compare(side, &a_node, &b_node);
    if report.skipped.is_some() {
        return Ok(unavailable("partial", "comparison-limit"));
    }
    let mut old = BTreeMap::new();
    let mut new = BTreeMap::new();
    crate::compatibility::index(&a_node.node, "$", false, &mut old);
    crate::compatibility::index(&b_node.node, "$", false, &mut new);
    let mut locations = BTreeMap::new();
    if !declared {
        for source in [a, b] {
            if let Parse::Parsed(value) =
                agw_inspector::inspect(source.body.as_bytes(), false, None).parse
            {
                concrete(&value, "$", "$", &mut locations);
            }
        }
        for paths in locations.values_mut() {
            paths.sort();
            paths.dedup();
        }
    }
    let findings=report.changes.iter().flat_map(|change| {
        let paths=if declared {vec![change.path.clone()]} else {locations.get(&change.path).cloned().unwrap_or_else(||vec![change.path.clone()])};
        paths.into_iter().map(|path|json!({"path":path,"pathKind":if declared{"schema"}else{"concrete"},"kind":format!("{:?}",change.kind),"class":change.class.to_string(),
            "compatibility":if declared{crate::compatibility::label(change.kind,old.get(&change.path).copied(),new.get(&change.path).copied(),side==Side::Request)}else{"unknown"},
            "provenance":{"baseline":a.provenance,"current":b.provenance,"kind":a.kind}})).collect::<Vec<_>>()
    }).collect::<Vec<_>>();
    Ok(json!({"coverage":if declared{"complete"}else{"partial"},"gaps":if declared{vec![]}else{vec!["observed-presence-only","unseen-variants"]},
        "requiredness":if declared{"declared"}else{"unknown"},"findings":findings}).to_string())
}
