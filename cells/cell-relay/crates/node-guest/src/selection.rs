use agw_diff::Side;
use agw_guard::statistical::{Engine, Field, Identity, Plan};
use agw_inspector::{Precision, inspect};
use agw_stats::windows::{Limits, Projection, Scope, Selection, Store};
use chrono::{DateTime, Duration};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    pub tenant: String,
    pub connection: String,
    pub vendor: String,
    pub operation: String,
    pub segment: String,
    pub fields: Vec<Samples>,
    #[serde(default)]
    pub materiality: Option<f64>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Samples {
    pub side: String,
    pub path: String,
    pub baseline: Vec<String>,
    pub live: Vec<String>,
    pub enumeration: bool,
}
/// Setup-only exact local windows. No collection, pooling or remote selection.
pub fn engine(json: &str) -> Result<Engine, String> {
    if json == "null" {
        return Ok(Engine::NullEngine);
    }
    if json.len() > 2 * 1024 * 1024 {
        return Err("selection too large".into());
    }
    let input: Input = serde_json::from_str(json).map_err(|e| e.to_string())?;
    if input.fields.len() > 7 {
        return Err("at most seven fields".into());
    }
    let mut fields = Vec::new();
    for sample in input.fields {
        let side = match sample.side.as_str() {
            "request" => Side::Request,
            "response" => Side::Response,
            _ => return Err("invalid side".into()),
        };
        if sample.baseline.is_empty()
            || sample.live.is_empty()
            || sample.baseline.len() > 4096
            || sample.live.len() > 4096
        {
            return Err("invalid window size".into());
        }
        let scope = Scope {
            tenant: input.tenant.clone(),
            connection: input.connection.clone(),
            vendor: input.vendor.clone(),
            operation: input.operation.clone(),
            segment: input.segment.clone(),
            side,
            field: sample.path,
            window: "sdk-local".into(),
            generation: "sdk-v1".into(),
        };
        let selection = Selection {
            scope: scope.clone(),
            projection: Projection::ExactInteger(Precision::Observed),
        };
        let mut store = Store::new(Limits {
            samples: 4096,
            scopes: 1,
            bytes: 2 * 1024 * 1024,
            batch: 1,
            inspection_nodes: 4096,
            inspection_bytes: 1024 * 1024,
            minimum: 1,
        })
        .map_err(|e| e.to_string())?;
        for (i, samples) in [sample.baseline, sample.live].into_iter().enumerate() {
            for (at, bytes) in samples.into_iter().enumerate() {
                store
                    .record(
                        std::slice::from_ref(&selection),
                        &inspect(bytes.as_bytes(), false, None),
                        DateTime::UNIX_EPOCH + Duration::seconds(at as i64),
                    )
                    .map_err(|e| e.to_string())?;
            }
            if i == 0 {
                store.publish(&scope).map_err(|e| e.to_string())?;
            }
        }
        fields.push(Field {
            snapshot: store.snapshot(&scope),
            scope,
            enumeration: sample.enumeration,
        });
    }
    let materiality = input.materiality.unwrap_or(0.2);
    if !materiality.is_finite() || materiality <= 0.0 || materiality > 1.0 {
        return Err("invalid materiality".into());
    }
    let mut plan = Plan::new(
        Identity {
            tenant: input.tenant,
            connection: input.connection,
            vendor: input.vendor,
            operation: input.operation,
            segment: input.segment,
        },
        fields,
    )
    .map_err(|e| e.to_string())?;
    plan.materiality = materiality;
    Ok(Engine::statistical(plan))
}
