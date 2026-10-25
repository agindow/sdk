//! TI-39: owned JSON values whose numbers never pass through a float.
//!
//! Deliberately no generic `Deserialize` implementation: parsing requires an
//! explicit precision decision via [`crate::inspect`].
use std::fmt;

use serde::de::{Error, MapAccess, Visitor};
use serde::ser::{Serialize, SerializeMap, Serializer};
use serde::{Deserialize, Deserializer};
use serde_json::value::RawValue;

use crate::{Number, ParseFailure, precision::Policy};

/// Lossless numeric JSON tree. Object entries retain duplicates so parsing
/// cannot silently discard a numeric token. Whole-body byte identity is not
/// promised: string escapes and whitespace may normalize on serialization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Null,
    Bool(bool),
    Number(Number),
    String(String),
    Array(Vec<Self>),
    Object(Vec<(String, Self)>),
}

impl Value {
    /// Parse raw JSON tokens without constructing serde_json numeric values.
    /// The policy supplies either a whole-document or exact field decision.
    pub(crate) fn from_slice(bytes: &[u8], precision: Policy<'_>) -> Result<Self, ParseFailure> {
        let raw: &RawValue = serde_json::from_slice(bytes).map_err(|e| ParseFailure::from(&e))?;
        Self::read(raw, precision, 0, bytes, &mut String::new())
    }

    fn read(
        raw: &RawValue,
        precision: Policy<'_>,
        depth: usize,
        source: &[u8],
        path: &mut String,
    ) -> Result<Self, ParseFailure> {
        // RawValue validates syntax without building a recursive tree. Bound
        // our own recursion as well, including subsequent clone/drop/serialize.
        if depth >= 128 {
            return Err(located(
                source,
                raw,
                serde_json::Error::custom("inspection nesting limit exceeded"),
            ));
        }
        let text = raw.get();
        match text.as_bytes().first() {
            Some(b'n') => Ok(Self::Null),
            Some(b't') => Ok(Self::Bool(true)),
            Some(b'f') => Ok(Self::Bool(false)),
            Some(b'"') => serde_json::from_str(text)
                .map(Self::String)
                .map_err(|e| located(source, raw, e)),
            Some(b'[') => {
                let array: Vec<&RawValue> =
                    serde_json::from_str(text).map_err(|e| located(source, raw, e))?;
                array
                    .into_iter()
                    .map(|value| {
                        let length = path.len();
                        precision.push_items(path);
                        let result = Self::read(value, precision, depth + 1, source, path);
                        path.truncate(length);
                        result
                    })
                    .collect::<Result<Vec<_>, _>>()
                    .map(Self::Array)
            }
            Some(b'{') => {
                let RawObject(entries) =
                    serde_json::from_str(text).map_err(|e| located(source, raw, e))?;
                entries
                    .into_iter()
                    .map(|(key, value)| {
                        // Reuse one path buffer: long ancestor keys must not be
                        // copied once per recursive level of the JSON tree.
                        let length = path.len();
                        precision.push_child(path, &key);
                        let result = Self::read(value, precision, depth + 1, source, path);
                        path.truncate(length);
                        result.map(|value| (key, value))
                    })
                    .collect::<Result<Vec<_>, _>>()
                    .map(Self::Object)
            }
            // RawValue already validated JSON grammar. Numeric tokens arrive
            // verbatim, including huge exponents, long decimals and -0.
            _ => Ok(Self::Number(Number::read(text, precision.at(path)))),
        }
    }
}

// Nested raw slices borrow from the original capture. Translate a string/key
// decoding error's local coordinates back into that capture, without ever
// interpreting a numeric token. No pointer is dereferenced or reconstructed.
fn located(source: &[u8], raw: &RawValue, error: serde_json::Error) -> ParseFailure {
    let mut failure = ParseFailure::from(&error);
    let offset = raw.get().as_ptr().addr() - source.as_ptr().addr();
    let prefix = &source[..offset];
    if failure.line == 0 {
        failure.line = 1;
        failure.column = 1;
    }
    if failure.line == 1 {
        failure.column += prefix
            .iter()
            .rposition(|b| *b == b'\n')
            .map_or(prefix.len(), |last| prefix.len() - last - 1);
    }
    failure.line += prefix.iter().filter(|b| **b == b'\n').count();
    if let Some((message, _)) = failure.message.rsplit_once(" at line ") {
        failure.message = format!(
            "{message} at line {} column {}",
            failure.line, failure.column
        );
    }
    failure
}

impl Serialize for Value {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Null => serializer.serialize_unit(),
            Self::Bool(value) => serializer.serialize_bool(*value),
            Self::Number(value) => value.serialize(serializer),
            Self::String(value) => serializer.serialize_str(value),
            Self::Array(values) => values.serialize(serializer),
            Self::Object(entries) => {
                let mut map = serializer.serialize_map(Some(entries.len()))?;
                for (key, value) in entries {
                    map.serialize_entry(key, value)?;
                }
                map.end()
            }
        }
    }
}

// A normal map would collapse duplicate keys before we could inspect their
// values. Borrow raw values in wire order, decoding only the property names.
pub(crate) struct RawObject<'a>(pub(crate) Vec<(String, &'a RawValue)>);

impl<'de> Deserialize<'de> for RawObject<'de> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Entries;
        impl<'de> Visitor<'de> for Entries {
            type Value = RawObject<'de>;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a JSON object")
            }

            fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
                let mut entries = Vec::new();
                while let Some(key) = map.next_key::<String>()? {
                    entries.push((key, map.next_value::<&RawValue>()?));
                }
                Ok(RawObject(entries))
            }
        }
        deserializer.deserialize_map(Entries)
    }
}
