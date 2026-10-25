#![allow(dead_code)]
use agw_diff::Side;
use agw_guard::{
    invariant::Selection,
    statistical::{Engine, Identity, Plan},
};
use agw_invariants::{
    Form, Relation, Rule,
    store::{Controls, Limits, Scope, Store},
};
pub type Error = Box<dyn std::error::Error>;
pub fn identity() -> Identity {
    Identity {
        tenant: "tenant".into(),
        connection: "conn".into(),
        vendor: "vendor".into(),
        operation: "endpoint".into(),
        segment: "segment".into(),
    }
}
pub fn selection() -> Result<Selection, Error> {
    let forms = [
        Form::Sum {
            total: "/total".into(),
            terms: vec!["/amount".into(), "/tax".into()],
            tolerance: 1,
        },
        Form::Range {
            field: "/total".into(),
            min: 0,
            max: 1000,
        },
        Form::Relation {
            left: "/total".into(),
            relation: Relation::GreaterEqual,
            right: "/amount".into(),
        },
        Form::Ordering {
            earlier: "/created".into(),
            later: "/updated".into(),
        },
        Form::Enum {
            field: "/status".into(),
            values: vec!["paid".into(), "pending".into()],
        },
        Form::CrossEndpoint {
            left: "/total".into(),
            right: "/correlated_total".into(),
            tolerance: 1,
        },
    ];
    let rules: Vec<_> = forms
        .into_iter()
        .enumerate()
        .map(|(i, form)| Rule {
            id: format!("rule-{i}"),
            form,
        })
        .collect();
    select(
        &rules,
        &[
            "/total",
            "/amount",
            "/tax",
            "/created",
            "/updated",
            "/status",
            "/correlated_total",
        ],
        Controls::default(),
    )
}
pub fn select(rules: &[Rule], bindings: &[&str], controls: Controls) -> Result<Selection, Error> {
    let mut store = Store::new(Limits::default())?;
    let scope = Scope {
        vendor: "vendor".into(),
        operation: "endpoint".into(),
        contract_version: 7,
    };
    store.publish(scope.clone(), 2, rules, bindings)?;
    Ok(Selection::new(store.select(&scope, 2, &controls)?)?)
}
pub fn engine() -> Result<Engine, Error> {
    Ok(Engine::statistical(
        Plan::new(identity(), vec![])?
            .with_invariants(Side::Request, selection()?)?
            .with_invariants(Side::Response, selection()?)?,
    ))
}
pub const CLEAN: &[u8] = br#"{ "total":120, "amount":100, "tax":20, "created":1, "updated":2, "status":"paid", "correlated_total":120 }"#;
pub const BAD: &[u8] = br#"{ "total":118, "amount":100, "tax":20, "created":1, "updated":2, "status":"paid", "correlated_total":118 }"#;
pub const NOVEL: &[u8] = br#"{ "total":120, "amount":100, "tax":20, "created":1, "updated":2, "status":"refunded", "correlated_total":120 }"#;
