use super::{Error, publication_identity};
use agw_invariants::cross_endpoint::{Endpoint, Key, Observation, Scope, Side};
use serde_json::Value;
use sha2::{Digest, Sha256};
pub(super) struct Parsed {
    pub id: String,
    pub job: String,
    pub recipe_id: String,
    pub revision: u32,
    pub attempt: u8,
    pub source: String,
    pub expires: i64,
    pub hash: Vec<u8>,
    pub observation: Observation,
}
pub(super) fn label(value: &str) -> Result<(), Error> {
    if value.is_empty() || value.chars().count() > 256 || value.chars().any(char::is_control) {
        Err(Error::Invalid)
    } else {
        Ok(())
    }
}
pub(super) fn authority(value: &str) -> Result<(), Error> {
    let url = url::Url::parse(value).map_err(|_| Error::Invalid)?;
    if value.len() > 2048
        || url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(Error::Invalid);
    }
    for (key, _) in url.query_pairs() {
        let key = key.to_lowercase();
        if [
            "token",
            "secret",
            "password",
            "credential",
            "authorization",
            "key",
        ]
        .iter()
        .any(|part| key.contains(part))
        {
            return Err(Error::Invalid);
        }
    }
    Ok(())
}
fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str, Error> {
    value.get(key).and_then(Value::as_str).ok_or(Error::Invalid)
}
fn number(value: &Value, key: &str) -> Result<u64, Error> {
    let raw = text(value, key)?;
    let number = raw.parse::<u64>().map_err(|_| Error::Invalid)?;
    if number == 0 || number.to_string() != raw {
        return Err(Error::Invalid);
    }
    Ok(number)
}
fn millis(value: &Value, key: &str) -> Result<i64, Error> {
    let raw = text(value, key)?;
    if !raw.ends_with('Z') {
        return Err(Error::Invalid);
    }
    let time = chrono::DateTime::parse_from_rfc3339(raw).map_err(|_| Error::Invalid)?;
    if time.timestamp_subsec_nanos() % 1000000 != 0 {
        return Err(Error::Invalid);
    }
    Ok(time.timestamp_millis())
}
pub(super) fn parse(bytes: &[u8], now: i64) -> Result<Parsed, Error> {
    if bytes.len() > 8192 {
        return Err(Error::Capacity);
    }
    let _: agw_audit::contracts::peer_observation::PeerObservation =
        serde_json::from_slice(bytes).map_err(|_| Error::Invalid)?;
    let value: Value = serde_json::from_slice(bytes).map_err(|_| Error::Invalid)?;
    let revision = u32::try_from(value["recipe_revision"].as_u64().ok_or(Error::Invalid)?)
        .map_err(|_| Error::Invalid)?;
    let attempt = u8::try_from(value["attempt"].as_u64().ok_or(Error::Invalid)?)
        .map_err(|_| Error::Invalid)?;
    if revision == 0 || revision > 2147483647 || !(1..=3).contains(&attempt) {
        return Err(Error::Invalid);
    }
    let sequence = number(&value, "sequence")?;
    let expires = millis(&value, "expires_at")?;
    let at_ms = millis(&value, "observed_at")?;
    if expires <= now
        || at_ms > now
        || i128::from(now) - i128::from(at_ms) > 300000
        || expires <= at_ms
        || i128::from(expires) - i128::from(at_ms) > 600000
    {
        return Err(Error::Expired);
    }
    let available = text(&value, "status")? == "available";
    let scalar = if available {
        let raw = text(&value, "value")?;
        let scalar = raw.parse::<i64>().map_err(|_| Error::Invalid)?;
        if scalar.to_string() != raw || !text(&value, "reason")?.is_empty() {
            return Err(Error::Invalid);
        }
        Some(scalar)
    } else {
        if !value["value"].is_null() || text(&value, "reason")?.is_empty() {
            return Err(Error::Invalid);
        }
        None
    };
    let scope = Scope {
        tenant: text(&value, "tenant")?.into(),
        connection: text(&value, "connection")?.into(),
        vendor: text(&value, "vendor")?.into(),
        workflow: text(&value, "workflow")?.into(),
        entity: text(&value, "entity")?.into(),
        revision: text(&value, "entity_revision")?.into(),
    };
    let key = Key {
        endpoint: Endpoint {
            operation: text(&value, "operation")?.into(),
            side: Side::Response,
            field: text(&value, "field")?.into(),
        },
        concept: text(&value, "concept")?.into(),
        unit: text(&value, "unit")?.into(),
        contract_version: number(&value, "contract_version")?,
    };
    scope.validate().map_err(Error::Store)?;
    key.validate().map_err(Error::Store)?;
    let id = text(&value, "id")?;
    let job = text(&value, "job_id")?;
    let recipe_id = text(&value, "recipe_id")?;
    if id != publication_identity(&scope.tenant, job, recipe_id, revision, sequence)? {
        return Err(Error::Invalid);
    }
    let source = text(&value, "source_url")?;
    authority(source)?;
    Ok(Parsed {
        id: id.into(),
        job: job.into(),
        recipe_id: recipe_id.into(),
        revision,
        attempt,
        source: source.into(),
        expires,
        hash: Sha256::digest(serde_json::to_vec(&value).map_err(|_| Error::Invalid)?).to_vec(),
        observation: Observation {
            scope,
            key,
            value: scalar,
            at_ms,
            sequence,
        },
    })
}
