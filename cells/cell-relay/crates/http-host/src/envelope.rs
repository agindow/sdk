use agw_guard::{config::Mode, statistical::Operation};
use serde::Deserialize;
use serde_json::{Map, Value, value::RawValue};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Envelope {
    pub tool: String,
    pub connection: String,
    pub operation: Operation,
    pub args: Box<RawValue>,
    pub mode: Mode,
    pub idempotency_key: Option<String>,
    pub durability: Option<String>,
    #[serde(default)]
    pub metadata: Map<String, Value>,
}
fn identifier(value: &str) -> bool {
    !value.is_empty() && value.len() <= 256 && !value.chars().any(char::is_control)
}
impl Envelope {
    pub fn validate(&self) -> Result<(), &'static str> {
        if !identifier(&self.tool) || !identifier(&self.connection) {
            return Err("tool and connection must be bounded identifiers");
        }
        if self.operation != Operation::Read
            && self
                .idempotency_key
                .as_deref()
                .is_none_or(|key| !identifier(key))
        {
            return Err("write and unknown require an upstream idempotency key");
        }
        if self
            .idempotency_key
            .as_deref()
            .is_some_and(|key| !identifier(key))
        {
            return Err("invalid idempotency key");
        }
        if self.args.get().len() > 8 * 1024 * 1024 {
            return Err("args exceed inspection bound");
        }
        if self.durability.is_some() {
            return Err("no implicit durability binding is supported");
        }
        Ok(())
    }
}
