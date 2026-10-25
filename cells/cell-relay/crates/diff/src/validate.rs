//! Validate caller-owned schemas before comparing: a limit is not vendor drift.
use crate::{Limits, Node, Skip};
use agw_inspector::inference::JsonType;

pub(crate) struct Budget {
    limits: Limits,
    nodes: usize,
    text: usize,
}
impl Budget {
    pub fn new(limits: Limits) -> Self {
        Self {
            limits,
            nodes: 0,
            text: 0,
        }
    }
    pub fn node(&mut self, depth: usize) -> Result<(), Skip> {
        self.nodes = self.nodes.saturating_add(1);
        if depth > self.limits.max_depth || self.nodes > self.limits.max_nodes {
            return Err(Skip::ResourceLimit);
        }
        Ok(())
    }
    pub fn text(&mut self, text: &str) -> Result<(), Skip> {
        self.text = self.text.saturating_add(text.len());
        if self.text > self.limits.max_text_bytes {
            return Err(Skip::ResourceLimit);
        }
        Ok(())
    }
}

pub(crate) fn validate(node: &Node, depth: usize, budget: &mut Budget) -> Result<(), Skip> {
    budget.node(depth)?;
    if node.types.is_empty()
        || ((!node.properties.is_empty() || !node.required.is_empty())
            && !node.types.contains(&JsonType::Object))
        || (node.items.is_some() && !node.types.contains(&JsonType::Array))
        || node
            .required
            .iter()
            .any(|key| !node.properties.contains_key(key))
        || node
            .enum_values
            .as_ref()
            .is_some_and(|values| values.is_empty())
    {
        return Err(Skip::InvalidSchema);
    }
    for text in node
        .units
        .iter()
        .chain(&node.identity)
        .chain(&node.null_meaning)
        .chain(node.required.iter())
        .chain(node.enum_values.iter().flatten())
    {
        budget.text(text)?;
    }
    for (key, node) in &node.properties {
        budget.text(key)?;
        validate(node, depth + 1, budget)?;
    }
    if let Some(items) = &node.items {
        validate(items, depth + 1, budget)?;
    }
    Ok(())
}
