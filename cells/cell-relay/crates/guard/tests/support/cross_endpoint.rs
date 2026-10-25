#![allow(dead_code)]
use agw_guard::{
    cross_endpoint::Selection,
    statistical::{Engine, Plan},
};
use agw_invariants::cross_endpoint::{
    Endpoint, Key, Limits, Observation, Rule, Scope, Side, Snapshot, Store,
};
pub type Error = Box<dyn std::error::Error>;
pub fn scope() -> Scope {
    Scope {
        tenant: "tenant".into(),
        connection: "conn".into(),
        vendor: "vendor".into(),
        workflow: "workflow".into(),
        entity: "charge-1".into(),
        revision: "revision-1".into(),
    }
}
pub fn endpoint(operation: &str, side: Side) -> Endpoint {
    Endpoint {
        operation: operation.into(),
        side,
        field: "/amount".into(),
    }
}
pub fn rule() -> Rule {
    Rule {
        id: "gross-agrees".into(),
        left: endpoint("charge", Side::Response),
        right: endpoint("endpoint", Side::Request),
        concept: "gross".into(),
        unit: "cent".into(),
        tolerance: 1,
        max_age_ms: 300_000,
        max_skew_ms: 120_000,
    }
}
pub fn observation(value: Option<i64>, at_ms: i64) -> Observation {
    Observation {
        scope: scope(),
        key: Key {
            endpoint: endpoint("charge", Side::Response),
            concept: "gross".into(),
            unit: "cent".into(),
            contract_version: 7,
        },
        value,
        at_ms,
        sequence: 1,
    }
}
pub fn snapshot(value: Option<i64>, at_ms: i64) -> Result<Snapshot, Error> {
    let store = Store::new(Limits::default())?;
    store.record(&[observation(value, at_ms)])?;
    Ok(store.snapshot(&scope())?)
}
pub fn selection() -> Result<Selection, Error> {
    Ok(Selection::new(2, 7, vec![rule()])?)
}
pub fn engine() -> Result<Engine, Error> {
    let identity = agw_guard::statistical::Identity {
        tenant: "tenant".into(),
        connection: "conn".into(),
        vendor: "vendor".into(),
        operation: "endpoint".into(),
        segment: "segment".into(),
    };
    Ok(Engine::statistical(
        Plan::new(identity, vec![])?.with_cross_endpoint(selection()?)?,
    ))
}
