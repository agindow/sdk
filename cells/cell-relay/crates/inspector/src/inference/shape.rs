//! Read raw JSON tokens: numeric magnitude and precision never pass through f64.
use std::collections::BTreeMap;

use crate::value::RawObject;
use serde_json::value::RawValue;

use super::{JsonType, LearnError, Limit, Limits, SpecPrior};

pub(super) fn parse(bytes: &[u8], limits: Limits) -> Result<SpecPrior, LearnError> {
    if bytes.len() > limits.max_bytes {
        return Err(LearnError::Limit(Limit::Bytes));
    }
    let raw: &RawValue = serde_json::from_slice(bytes).map_err(malformed)?;
    let mut nodes = 0;
    read(raw, 0, &mut nodes, limits)
}

fn malformed(error: serde_json::Error) -> LearnError {
    LearnError::Malformed(error.to_string())
}

fn read(
    raw: &RawValue,
    depth: usize,
    nodes: &mut usize,
    limits: Limits,
) -> Result<SpecPrior, LearnError> {
    if depth > limits.max_depth {
        return Err(LearnError::Limit(Limit::Depth));
    }
    // Count parsed nodes as well as merged nodes, so a huge repeated array
    // cannot spend unlimited work while producing only a tiny schema.
    *nodes += 1;
    if *nodes > limits.max_nodes {
        return Err(LearnError::Limit(Limit::Nodes));
    }
    let text = raw.get();
    let shape = match text.as_bytes().first() {
        Some(b'{') => {
            // Validate and count overwritten values too: collecting raw members
            // into a map first would let duplicate keys evade learning limits.
            let RawObject(object) = serde_json::from_str(text).map_err(malformed)?;
            let properties = object
                .into_iter()
                .map(|(key, value)| read(value, depth + 1, nodes, limits).map(|shape| (key, shape)))
                .collect::<Result<BTreeMap<_, _>, _>>()?;
            SpecPrior::object(properties)
        }
        Some(b'[') => {
            let array: Vec<&RawValue> = serde_json::from_str(text).map_err(malformed)?;
            let mut items: Option<SpecPrior> = None;
            for value in array {
                let shape = read(value, depth + 1, nodes, limits)?;
                items = Some(match items {
                    Some(existing) => existing.union(shape),
                    None => shape,
                });
            }
            match items {
                Some(items) => SpecPrior::array(items),
                None => SpecPrior::scalar(JsonType::Array),
            }
        }
        Some(b'"') => {
            // RawValue skips surrogate validation; match inspection's decoded
            // string boundary without routing numeric tokens through a float.
            let _: String = serde_json::from_str(text).map_err(malformed)?;
            SpecPrior::scalar(JsonType::String)
        }
        Some(b't' | b'f') => SpecPrior::scalar(JsonType::Boolean),
        Some(b'n') => SpecPrior::scalar(JsonType::Null),
        _ => {
            let mut shape = SpecPrior::scalar(if text.contains(['.', 'e', 'E']) {
                JsonType::Number
            } else {
                JsonType::Integer
            });
            shape.overflow = matches!(
                crate::Number::read(text, crate::Precision::Observed),
                crate::Number::Unrepresentable(_)
            );
            shape
        }
    };
    Ok(shape)
}

pub(super) fn validate_prior(
    prior: &SpecPrior,
    depth: usize,
    limits: Limits,
) -> Result<usize, LearnError> {
    if depth > limits.max_depth {
        return Err(LearnError::Limit(Limit::Depth));
    }
    let mut nodes = 1;
    for node in prior.properties.values().chain(prior.items.as_deref()) {
        nodes += validate_prior(node, depth + 1, limits)?;
        if nodes > limits.max_nodes {
            return Err(LearnError::Limit(Limit::Nodes));
        }
    }
    if nodes > limits.max_nodes {
        return Err(LearnError::Limit(Limit::Nodes));
    }
    Ok(nodes)
}
