//! Typed evidence supplied by the selected baseline and detector callers.
use std::collections::{BTreeMap, BTreeSet};

use agw_inspector::inference::{JsonType, Schema};

use crate::DriftClass;

/// A schema node, not an arbitrary JSON Schema evaluator. Enum values are
/// caller-normalized tokens; string enums can use their exact decoded values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    pub types: BTreeSet<JsonType>,
    pub properties: BTreeMap<String, Self>,
    pub items: Option<Box<Self>>,
    pub required: BTreeSet<String>,
    pub units: Option<String>,
    pub enum_values: Option<BTreeSet<String>>,
    pub null_meaning: Option<String>,
    /// Confirmed identity, not a name similarity or a candidate match.
    pub identity: Option<String>,
}

impl Node {
    pub fn scalar(kind: JsonType) -> Self {
        Self {
            types: BTreeSet::from([kind]),
            properties: BTreeMap::new(),
            items: None,
            required: BTreeSet::new(),
            units: None,
            enum_values: None,
            null_meaning: None,
            identity: None,
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
}

/// Converting a learner result supplies no requiredness or semantic facts.
impl From<&Schema> for Node {
    fn from(schema: &Schema) -> Self {
        Self {
            types: schema.types.clone(),
            properties: schema
                .properties
                .iter()
                .map(|(k, v)| (k.clone(), Self::from(v)))
                .collect(),
            items: schema.items.as_deref().map(Self::from).map(Box::new),
            ..Self::scalar(JsonType::Null)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Evidence {
    Authoritative,
    Learned,
    Observed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub node: Node,
    pub evidence: Evidence,
}

impl From<&Schema> for Snapshot {
    fn from(schema: &Schema) -> Self {
        Self {
            node: Node::from(schema),
            evidence: Evidence::Learned,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Request,
    Response,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Removed,
    Added,
    OptionalAdded,
    RequiredAdded,
    RequiredChanged,
    Renamed,
    TypeChanged,
    UnitsChanged,
    EnumExpanded,
    EnumChanged,
    NullMeaningChanged,
    MalformedBody,
}

/// Stable, machine-readable explanation; consumers need not parse prose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    DeclaredStructure,
    OptionalInformation,
    ConfirmedIdentity,
    AmbiguousIdentity,
    InsufficientEvidence,
    NumericCoercion,
    NullabilityChanged,
    UnitsChanged,
    EnumExpanded,
    MeaningChanged,
    MetadataLost,
    EnumConstraintChanged,
    CompleteMalformedBody,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub side: Side,
    pub path: String,
    pub to_path: Option<String>,
    pub kind: Kind,
    pub class: DriftClass,
    pub reason: Reason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Skip {
    Incomplete,
    ParserFailure,
    AmbiguousObject,
    InvalidSchema,
    ResourceLimit,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    pub changes: Vec<Change>,
    pub skipped: Option<Skip>,
}

impl Report {
    /// Semantic evidence must survive even alongside an uncertain finding.
    pub fn class(&self) -> Option<DriftClass> {
        self.changes
            .iter()
            .map(|c| c.class)
            .max_by_key(|class| match class {
                DriftClass::Shape => 0,
                DriftClass::Unknown => 1,
                DriftClass::Semantic => 2,
            })
    }

    pub(crate) fn skip(reason: Skip) -> Self {
        Self {
            skipped: Some(reason),
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub max_depth: usize,
    pub max_nodes: usize,
    pub max_text_bytes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_depth: 64,
            max_nodes: 4096,
            max_text_bytes: 1024 * 1024,
        }
    }
}

pub(crate) fn child(path: &str, key: &str) -> String {
    let escaped = if key.is_empty() {
        "\\e".into()
    } else {
        key.chars()
            .flat_map(|c| {
                if matches!(c, '\\' | '.' | '[' | ']' | '$') {
                    vec!['\\', c]
                } else {
                    vec![c]
                }
            })
            .collect::<String>()
    };
    if path == "$" {
        escaped
    } else {
        format!("{path}.{escaped}")
    }
}
