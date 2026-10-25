//! Lossless inspection adapter. Never serialize or rewrite customer bytes.
use std::collections::BTreeSet;

use crate::validate::{Budget, validate};
use crate::{
    Change, DriftClass, Evidence, Kind, Limits, Node, Reason, Report, Side, Skip, Snapshot,
    compare_with_limits,
};
use agw_inspector::{Inspection, Parse, ParseClass, Value, inference::JsonType as T};

pub fn compare_inspection(side: Side, baseline: &Snapshot, inspection: &Inspection) -> Report {
    compare_inspection_with_limits(side, baseline, inspection, Limits::default())
}

pub fn compare_inspection_with_limits(
    side: Side,
    baseline: &Snapshot,
    inspection: &Inspection,
    limits: Limits,
) -> Report {
    if inspection.truncated {
        return Report::skip(Skip::Incomplete);
    }
    if let Err(skip) = validate(&baseline.node, 0, &mut Budget::new(limits)) {
        return Report::skip(skip);
    }
    let value = match &inspection.parse {
        Parse::Parsed(value) => value,
        Parse::Unparseable(error) => {
            return match error.class {
                ParseClass::Syntax | ParseClass::Eof => malformed(side),
                ParseClass::Data | ParseClass::Io => Report::skip(Skip::ParserFailure),
            };
        }
    };
    let observed = match shape(value, 0, &mut Budget::new(limits)) {
        Ok(node) => node,
        Err(skip) => return Report::skip(skip),
    };
    if baseline.evidence == Evidence::Authoritative && baseline.node.types.len() == 1 {
        let expected = &baseline.node.types;
        if (expected.contains(&T::Object) || expected.contains(&T::Array))
            && expected != &observed.types
        {
            return malformed(side);
        }
    }
    compare_with_limits(
        side,
        baseline,
        &Snapshot {
            node: observed,
            evidence: Evidence::Observed,
        },
        limits,
    )
}

fn malformed(side: Side) -> Report {
    Report {
        changes: vec![Change {
            side,
            path: "$".into(),
            to_path: None,
            kind: Kind::MalformedBody,
            class: DriftClass::Shape,
            reason: Reason::CompleteMalformedBody,
        }],
        skipped: None,
    }
}

fn shape(value: &Value, depth: usize, budget: &mut Budget) -> Result<Node, Skip> {
    budget.node(depth)?;
    Ok(match value {
        Value::Null => Node::scalar(T::Null),
        Value::Bool(_) => Node::scalar(T::Boolean),
        Value::String(_) => Node::scalar(T::String),
        Value::Number(number) => Node::scalar(if number.lexeme().contains(['.', 'e', 'E']) {
            T::Number
        } else {
            T::Integer
        }),
        Value::Object(entries) => {
            let mut keys = BTreeSet::new();
            let mut node = Node::scalar(T::Object);
            for (key, value) in entries {
                budget.text(key)?;
                if !keys.insert(key) {
                    return Err(Skip::AmbiguousObject);
                }
                node.properties
                    .insert(key.clone(), shape(value, depth + 1, budget)?);
            }
            node
        }
        Value::Array(values) => {
            let mut node = Node::scalar(T::Array);
            for value in values {
                let next = shape(value, depth + 1, budget)?;
                if let Some(items) = &mut node.items {
                    merge(items, next);
                } else {
                    node.items = Some(Box::new(next));
                }
            }
            node
        }
    })
}

fn merge(node: &mut Node, other: Node) {
    node.types.extend(other.types);
    for (key, next) in other.properties {
        if let Some(existing) = node.properties.get_mut(&key) {
            merge(existing, next);
        } else {
            node.properties.insert(key, next);
        }
    }
    if let Some(next) = other.items {
        if let Some(items) = &mut node.items {
            merge(items, *next);
        } else {
            node.items = Some(next);
        }
    }
}
