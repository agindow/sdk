//! Structural evidence; no payload values or repair confidence are retained.
use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value, json};

/// JSON Schema type names, kept distinct even when a number is outside u64.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum JsonType {
    Array,
    Boolean,
    Integer,
    Null,
    Number,
    Object,
    String,
}

impl JsonType {
    fn name(self) -> &'static str {
        match self {
            Self::Array => "array",
            Self::Boolean => "boolean",
            Self::Integer => "integer",
            Self::Null => "null",
            Self::Number => "number",
            Self::Object => "object",
            Self::String => "string",
        }
    }
}

/// A structural prior extracted from an available spec by the caller.
/// Constructors describe shape only; constraints such as required/format/enum
/// are deliberately outside TI-10. A prior contributes one pseudo-sample.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecPrior {
    pub(super) overflow: bool,
    pub(super) types: BTreeSet<JsonType>,
    pub(super) properties: BTreeMap<String, Self>,
    pub(super) items: Option<Box<Self>>,
}

impl SpecPrior {
    /// A single JSON type. Object/array types initially have no child evidence.
    pub fn scalar(kind: JsonType) -> Self {
        Self {
            overflow: false,
            types: BTreeSet::from([kind]),
            properties: BTreeMap::new(),
            items: None,
        }
    }

    pub fn object(properties: impl IntoIterator<Item = (String, Self)>) -> Self {
        Self {
            properties: properties.into_iter().collect(),
            ..Self::scalar(JsonType::Object)
        }
    }

    pub fn array(items: Self) -> Self {
        Self {
            items: Some(Box::new(items)),
            ..Self::scalar(JsonType::Array)
        }
    }

    /// Union shapes, including heterogeneous array items, without weighting
    /// longer arrays more heavily than a single response.
    pub fn union(mut self, other: Self) -> Self {
        self.overflow |= other.overflow;
        self.types.extend(other.types);
        for (name, node) in other.properties {
            match self.properties.remove(&name) {
                Some(existing) => {
                    self.properties.insert(name, existing.union(node));
                }
                None => {
                    self.properties.insert(name, node);
                }
            }
        }
        self.items = match (self.items, other.items) {
            (Some(a), Some(b)) => Some(Box::new(a.union(*b))),
            (a, b) => a.or(b),
        };
        self
    }
}

/// An inferred node with coverage evidence. Confidence is sample presence,
/// not probability of safe repair and not conditional on parent presence.
#[derive(Debug, Clone, PartialEq)]
pub struct Schema {
    /// Inferred only after overflow in at least two complete live samples.
    pub high_precision: Option<bool>,
    pub overflow_samples: usize,
    pub types: BTreeSet<JsonType>,
    pub observations: usize,
    pub confidence: f64,
    pub properties: BTreeMap<String, Self>,
    pub items: Option<Box<Self>>,
}

impl Schema {
    /// Standard JSON Schema shape projection; no requiredness is inferred.
    pub fn json_schema(&self) -> Value {
        let mut value =
            json!({"type": self.types.iter().map(|kind| kind.name()).collect::<Vec<_>>()});
        if let Some(flag) = self.high_precision {
            value["high_precision"] = json!(flag);
        }
        if !self.properties.is_empty() {
            let properties: BTreeMap<_, _> = self
                .properties
                .iter()
                .map(|(name, node)| (name, node.json_schema()))
                .collect();
            value["properties"] = json!(properties);
        }
        if let Some(items) = &self.items {
            value["items"] = items.json_schema();
        }
        value
    }

    /// Legacy structural paths retained for TI-10 callers, including empty keys.
    pub fn paths(&self) -> BTreeMap<String, &Self> {
        let mut paths = BTreeMap::new();
        self.visit_legacy(None, &mut paths);
        paths
    }
    fn visit_legacy<'a>(&'a self, path: Option<String>, out: &mut BTreeMap<String, &'a Self>) {
        out.insert(path.clone().unwrap_or_else(|| "$".into()), self);
        for (key, node) in &self.properties {
            let key = key
                .chars()
                .flat_map(|c| {
                    if matches!(c, '\\' | '.' | '[' | ']' | '$') {
                        vec!['\\', c]
                    } else {
                        vec![c]
                    }
                })
                .collect::<String>();
            let next = path
                .as_ref()
                .map_or_else(|| key.clone(), |p| format!("{p}.{key}"));
            node.visit_legacy(Some(next), out);
        }
        if let Some(items) = &self.items {
            items.visit_legacy(Some(format!("{}[]", path.as_deref().unwrap_or("$"))), out);
        }
    }

    /// Precision paths use `\\e` for empty keys, matching snapshot readers.
    pub fn precision_paths(&self) -> BTreeMap<String, &Self> {
        let mut paths = BTreeMap::new();
        self.visit("$".into(), &mut paths);
        paths
    }
    fn visit<'a>(&'a self, path: String, out: &mut BTreeMap<String, &'a Self>) {
        out.insert(path.clone(), self);
        for (key, node) in &self.properties {
            node.visit(crate::precision::child(&path, key), out);
        }
        if let Some(items) = &self.items {
            items.visit(format!("{path}[]"), out);
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(super) struct Evidence {
    types: BTreeSet<JsonType>,
    presence: usize,
    overflow_samples: usize,
    properties: BTreeMap<String, Self>,
    items: Option<Box<Self>>,
}

impl Evidence {
    pub fn add(&mut self, shape: SpecPrior) {
        self.presence += 1;
        self.overflow_samples += usize::from(shape.overflow);
        self.types.extend(shape.types);
        for (key, node) in shape.properties {
            self.properties.entry(key).or_default().add(node);
        }
        if let Some(items) = shape.items {
            self.items.get_or_insert_default().add(*items);
        }
    }

    pub fn schema(&self, total: usize) -> Schema {
        Schema {
            high_precision: (self.overflow_samples >= 2).then_some(true),
            overflow_samples: self.overflow_samples,
            types: self.types.clone(),
            observations: self.presence,
            confidence: self.presence as f64 / total as f64,
            properties: self
                .properties
                .iter()
                .map(|(key, node)| (key.clone(), node.schema(total)))
                .collect(),
            items: self.items.as_ref().map(|node| Box::new(node.schema(total))),
        }
    }

    pub fn nodes(&self) -> usize {
        1 + self.properties.values().map(Self::nodes).sum::<usize>()
            + self.items.as_ref().map_or(0, |node| node.nodes())
    }
}
