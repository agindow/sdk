//! Serde adapters for TI-18 schemas that typify cannot emit directly.
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Fixed-length arrays above serde's built-in 32-element implementations.
#[derive(Clone, Debug)]
pub struct FixedArray<T, const N: usize>(pub [T; N]);

impl<T: Serialize, const N: usize> Serialize for FixedArray<T, N> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.as_slice().serialize(serializer)
    }
}

impl<'de, T: Deserialize<'de>, const N: usize> Deserialize<'de> for FixedArray<T, N> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let values = Vec::<T>::deserialize(deserializer)?;
        let length = values.len();
        values
            .try_into()
            .map(Self)
            .map_err(|_| serde::de::Error::custom(format!("expected {N} items, received {length}")))
    }
}

/// RFC 6901 pointer with the frozen schema's 1024-character ceiling.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct JsonPointer(String);

impl TryFrom<String> for JsonPointer {
    type Error = String;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.chars().count() > 1024 || (!value.is_empty() && !value.starts_with('/')) {
            return Err("invalid profile JSON Pointer".into());
        }
        let mut chars = value.chars();
        while let Some(ch) = chars.next() {
            if ch == '~' && !matches!(chars.next(), Some('0' | '1')) {
                return Err("invalid JSON Pointer escape".into());
            }
        }
        Ok(Self(value))
    }
}

impl From<JsonPointer> for String {
    fn from(value: JsonPointer) -> Self {
        value.0
    }
}

/// typify does not enforce integer `const`; keep the queue envelope version exact.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub struct ProfileVersion;

impl TryFrom<u8> for ProfileVersion {
    type Error = String;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        if value == 1 {
            Ok(Self)
        } else {
            Err("expected field_profile version 1".into())
        }
    }
}

impl From<ProfileVersion> for u8 {
    fn from(_: ProfileVersion) -> Self {
        1
    }
}

/// The shared nonempty, bounded profile identifier schema.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ProfileIdentifier(String);

impl ProfileIdentifier {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for ProfileIdentifier {
    type Error = String;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.is_empty() || value.chars().count() > 256 {
            return Err("profile identifier must have 1–256 characters".into());
        }
        Ok(Self(value))
    }
}

impl From<ProfileIdentifier> for String {
    fn from(value: ProfileIdentifier) -> Self {
        value.0
    }
}
