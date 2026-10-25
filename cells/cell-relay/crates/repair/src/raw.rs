//! Borrow children with a hard collection bound, before allocating an index.
use crate::Failure;
use serde::{
    Deserializer,
    de::{DeserializeSeed, Error, MapAccess, SeqAccess, Visitor},
};
use serde_json::value::RawValue;

pub(crate) enum Children<'a> {
    Object(Vec<(String, &'a RawValue)>),
    Array(Vec<&'a RawValue>),
}

pub(crate) fn children(raw: &RawValue, cap: usize) -> Result<Children<'_>, Failure> {
    let mut exceeded = false;
    let result = Bounded {
        cap,
        exceeded: &mut exceeded,
    }
    .deserialize(&mut serde_json::Deserializer::from_str(raw.get()));
    result.map_err(|_| {
        if exceeded {
            Failure::ResourceLimit
        } else {
            Failure::InspectionMismatch
        }
    })
}
struct Bounded<'a> {
    cap: usize,
    exceeded: &'a mut bool,
}
impl<'de> DeserializeSeed<'de> for Bounded<'_> {
    type Value = Children<'de>;
    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_any(self)
    }
}
impl<'de> Visitor<'de> for Bounded<'_> {
    type Value = Children<'de>;
    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("a bounded object or array")
    }
    fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
        let mut entries = Vec::new();
        while let Some(key) = map.next_key::<String>()? {
            if entries.len() >= self.cap {
                *self.exceeded = true;
                return Err(M::Error::custom("repair node limit"));
            }
            entries.push((key, map.next_value::<&RawValue>()?));
        }
        Ok(Children::Object(entries))
    }
    fn visit_seq<S: SeqAccess<'de>>(self, mut seq: S) -> Result<Self::Value, S::Error> {
        let mut entries = Vec::new();
        while let Some(raw) = seq.next_element::<&RawValue>()? {
            if entries.len() >= self.cap {
                *self.exceeded = true;
                return Err(S::Error::custom("repair node limit"));
            }
            entries.push(raw);
        }
        Ok(Children::Array(entries))
    }
}
