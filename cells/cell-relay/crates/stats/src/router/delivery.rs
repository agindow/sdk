//! Immutable host configuration copied into the local queue; no I/O or callbacks.
use super::Error;

#[derive(Debug, Clone)]
pub struct DeliveryContext {
    pub tenant: String,
    pub call_id: String,
    pub price_context: String,
    pub created_ms: i64,
}
impl DeliveryContext {
    /// Currency is explicitly USD in the frozen investigation-job v1 contract.
    pub fn new(
        tenant: &str,
        call_id: &str,
        price_context: &str,
        created_ms: i64,
    ) -> Result<Self, Error> {
        for value in [tenant, call_id, price_context] {
            identifier(value)?;
        }
        if chrono::DateTime::from_timestamp_millis(created_ms).is_none()
            || created_ms
                .checked_add(600000)
                .and_then(chrono::DateTime::from_timestamp_millis)
                .is_none()
        {
            return Err(Error::InvalidInput);
        }
        Ok(Self {
            tenant: tenant.into(),
            call_id: call_id.into(),
            price_context: price_context.into(),
            created_ms,
        })
    }
}
pub(super) fn identifier(value: &str) -> Result<(), Error> {
    if value.is_empty()
        || value.len() > 256
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_.:-".contains(&b))
    {
        Err(Error::InvalidInput)
    } else {
        Ok(())
    }
}
