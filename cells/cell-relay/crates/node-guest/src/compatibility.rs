//! Directional compatibility uses declared authority only; observed presence is unknown.
use agw_diff::{Kind, Node};
use agw_inspector::inference::JsonType as T;
use std::collections::BTreeMap;
pub(crate) fn index<'a>(
    node: &'a Node,
    path: &str,
    required: bool,
    map: &mut BTreeMap<String, (&'a Node, bool)>,
) {
    map.insert(path.into(), (node, required));
    for (key, node) in &node.properties {
        let key = crate::nested::escape(key);
        let next = if path == "$" {
            key.clone()
        } else {
            format!("{path}.{key}")
        };
        // Requiredness belongs to the parent, not an inferred child.
        let parent = map.get(path).map(|(n, _)| *n);
        let required =
            parent.is_some_and(|p| p.required.iter().any(|k| crate::nested::escape(k) == key));
        index(node, &next, required, map);
    }
    if let Some(items) = &node.items {
        index(items, &format!("{path}[]"), false, map);
    }
}
fn subset(a: &Node, b: &Node) -> bool {
    a.types
        .iter()
        .all(|t| b.types.contains(t) || (*t == T::Integer && b.types.contains(&T::Number)))
}
pub(crate) fn label(
    kind: Kind,
    a: Option<(&Node, bool)>,
    b: Option<(&Node, bool)>,
    request: bool,
) -> &'static str {
    let compatible = match kind {
        Kind::OptionalAdded => Some(true),
        Kind::RequiredAdded => Some(!request),
        Kind::Removed => Some(!request && a.is_some_and(|(_, required)| !required)),
        Kind::RequiredChanged => match (a, b) {
            (Some((_, old)), Some((_, new))) => {
                Some(if request { !new || old } else { !old || new })
            }
            _ => None,
        },
        Kind::TypeChanged => match (a, b) {
            (Some((old, _)), Some((new, _))) => Some(if request {
                subset(old, new)
            } else {
                subset(new, old)
            }),
            _ => None,
        },
        Kind::EnumChanged | Kind::EnumExpanded => match (a, b) {
            (Some((old, _)), Some((new, _))) => {
                let (accept, produce) = if request {
                    (&new.enum_values, &old.enum_values)
                } else {
                    (&old.enum_values, &new.enum_values)
                };
                match (accept, produce) {
                    (None, _) => Some(true),
                    (Some(_), None) => Some(false),
                    (Some(a), Some(b)) => Some(b.is_subset(a)),
                }
            }
            _ => None,
        },
        _ => None,
    };
    match compatible {
        Some(true) => "compatible",
        Some(false) => "breaking",
        None => "unknown",
    }
}
