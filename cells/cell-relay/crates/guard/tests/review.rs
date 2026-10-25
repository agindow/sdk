#[path = "support/idempotent_guard.rs"]
mod support;
use agw_diff::DriftClass as Class;
use agw_guard::{
    config::{Config, RuntimeConfig},
    gated::{Body, Call},
    readiness::AvailabilityGate,
    review::{self, Outbox, Scope},
};
use agw_policy::{Decision, Operation};
use std::sync::atomic::Ordering;
use support::{Engine, ResponseOnly, draft};
type Result = std::result::Result<(), Box<dyn std::error::Error>>;
fn scope() -> std::result::Result<Scope, Box<dyn std::error::Error>> {
    Ok(Scope {
        tenant: "acme".into(),
        connection: "conn_1".into(),
        vendor: "stripe".into(),
        operation: "create_customer".into(),
        contract_version: draft()?.proof.contract_version.to_string(),
        repair_id: "rename-source-v1".into(),
    })
}
#[test]
fn strict_halts_every_unresolved_class_and_operation_with_complete_proposals() -> Result {
    let config = RuntimeConfig::from_sources(Config::default(), None)?;
    config.reload_mode("strict")?;
    for op in [Operation::Read, Operation::Write, Operation::Unknown] {
        for class in [Class::Shape, Class::Semantic, Class::Unknown] {
            let gate = AvailabilityGate::new(true);
            let mut engine = Engine::new(Some(class));
            engine.propose = true;
            engine.confidence = 0.98;
            let mut audit = draft()?;
            audit.operation = op;
            let result: agw_guard::gated::Outcome<'_, ()> =
                Call::new(config.begin_call(None)?, &gate, 100_000).execute(
                    Body::complete(b"{\"old\":1}"),
                    &engine,
                    audit,
                    |_| panic!("strict halt must not dispatch"),
                );
            assert_eq!(result.request.decision, Decision::Halt);
            let case = review::case(&result, &scope()?)?.ok_or("missing escalation")?;
            let json = serde_json::to_value(case)?;
            assert_eq!(json["audit"]["decision"], "blocked");
            assert_eq!(json["dispatched"], false);
            assert_eq!(json["evidence"][0]["proposals"][0]["confidence"], 0.98);
            assert!(
                json["evidence"][0]["proposals"][0]["assessment"]
                    .as_str()
                    .is_some_and(|s| s.contains("Rejected"))
            );
            assert_eq!(engine.verifications.load(Ordering::SeqCst), 0);
        }
    }
    Ok(())
}
#[test]
fn strict_point99_requires_verification_and_only_halts_get_review_items() -> Result {
    for (confidence, verified, halt) in [
        (0.989999, true, true),
        (0.99, false, true),
        (0.99, true, false),
    ] {
        let config = RuntimeConfig::from_sources(Config::default(), None)?;
        config.reload_mode("strict")?;
        let gate = AvailabilityGate::new(true);
        let mut engine = Engine::new(Some(Class::Shape));
        engine.propose = true;
        engine.confidence = confidence;
        engine.verify = verified;
        let engine = RequestOnly(engine);
        let mut dispatched = 0;
        let result = Call::new(config.begin_call(None)?, &gate, 100_000).execute(
            Body::complete(b"{\"old\":1}"),
            &engine,
            draft()?,
            |_| {
                dispatched += 1;
                (17, Body::complete(b"{}"))
            },
        );
        assert_eq!(dispatched, usize::from(!halt));
        assert_eq!(review::case(&result, &scope()?)?.is_some(), halt);
        if confidence == 0.99 && !verified {
            let case = review::case(&result, &scope()?)?.ok_or("case")?;
            assert_eq!(
                serde_json::to_value(case)?["evidence"][0]["verification"],
                "failed"
            );
        }
    }
    Ok(())
}
#[test]
fn bounded_outbox_failure_never_releases_a_completed_response_halt() -> Result {
    let config = RuntimeConfig::from_sources(Config::default(), None)?;
    config.reload_mode("strict")?;
    let gate = AvailabilityGate::new(true);
    let engine = ResponseOnly(Engine::new(Some(Class::Semantic)));
    let result = Call::new(config.begin_call(None)?, &gate, 100_000).execute(
        Body::complete(b"{}"),
        &engine,
        draft()?,
        |_| (Box::new(17), Body::complete(b"{\"old\":1}")),
    );
    let mut outbox = Outbox::new("acme", 1, 128 * 1024)?;
    assert!(outbox.submit(&result, &scope()?)?);
    assert!(!outbox.submit(&result, &scope()?)?);
    assert!(outbox.ack("wrong").is_err());
    let id = outbox.front().ok_or("queued case")?.id.to_string();
    assert_eq!(**result.outcome.as_ref().ok_or("original outcome")?, 17);
    assert_eq!(
        result.response.as_ref().ok_or("response")?.decision,
        Decision::Halt
    );
    let mut other = scope()?;
    other.connection = "different".into();
    assert!(outbox.submit(&result, &other).is_err());
    outbox.ack(&id)?;
    assert!(outbox.front().is_none());
    Ok(())
}
#[test]
fn availability_is_not_human_escalation() -> Result {
    let config = RuntimeConfig::from_sources(Config::default(), None)?;
    config.reload_mode("strict")?;
    let gate = AvailabilityGate::new(false);
    let result = Call::new(config.begin_call(None)?, &gate, 100_000).execute(
        Body::complete(b"{}"),
        &Engine::new(Some(Class::Semantic)),
        draft()?,
        |_| (17, Body::complete(b"{}")),
    );
    assert_eq!(result.bypasses, 1);
    assert!(review::case(&result, &scope()?)?.is_none());
    Ok(())
}
struct RequestOnly(Engine);
impl agw_guard::gated::Pipeline for RequestOnly {
    fn detect(
        &self,
        side: agw_diff::Side,
        bytes: &[u8],
        budget: &mut agw_guard::availability::Budget,
    ) -> std::result::Result<agw_guard::gated::Evidence, agw_guard::availability::EngineFault> {
        if side == agw_diff::Side::Response {
            Ok(agw_guard::gated::Evidence::default())
        } else {
            agw_guard::gated::Pipeline::detect(&self.0, side, bytes, budget)
        }
    }
    fn verify(
        &self,
        side: agw_diff::Side,
        bytes: &[u8],
        findings: &[usize],
        budget: &mut agw_guard::availability::Budget,
    ) -> std::result::Result<bool, agw_guard::availability::EngineFault> {
        agw_guard::gated::Pipeline::verify(&self.0, side, bytes, findings, budget)
    }
}
#[path = "support/idempotency.rs"]
mod cache;
#[test]
fn terminal_replay_preserves_full_escalation_without_dispatch_or_new_inspection() -> Result {
    let config = RuntimeConfig::from_sources(Config::default(), None)?;
    config.reload_mode("strict")?;
    let store = cache::store::<agw_guard::idempotency::Completion<()>>(
        2,
        std::time::Duration::from_secs(60),
    )?;
    let gate = AvailabilityGate::new(true);
    let mut engine = Engine::new(Some(Class::Semantic));
    engine.propose = true;
    engine.confidence = 0.98;
    let first = cache::completed(
        Call::new(config.begin_call(None)?, &gate, 100_000).execute_once(
            &store,
            cache::request("conn", Some("key")),
            Body::complete(b"{\"old\":1}"),
            &engine,
            draft()?,
            |_, _| panic!("halt"),
        )?,
    )?;
    let replay = cache::completed(Call::new(config.begin_call(None)?, &gate, 0).execute_once(
        &store,
        cache::request("conn", Some("key")),
        Body::complete(b"changed"),
        &engine,
        draft()?,
        |_, _| panic!("terminal"),
    )?)?;
    assert!(std::sync::Arc::ptr_eq(&first.value, &replay.value));
    let case = review::case_from_completion(&replay.value, &scope()?)?.ok_or("case")?;
    assert_eq!(
        serde_json::to_value(&case)?["evidence"][0]["proposals"][0]["replacement_hex"],
        "32"
    );
    let mut outbox = Outbox::new("acme", 1, 1)?;
    assert_eq!(
        outbox.submit_completion(&replay.value, &scope()?),
        Err(review::Error::Full)
    );
    assert_eq!(replay.value.audit.decision.to_string(), "blocked");
    assert!(replay.value.outcome.is_none());
    assert_eq!(engine.evaluations.load(Ordering::SeqCst), 1);
    Ok(())
}
#[test]
fn shared_review_envelopes_deserialize_with_frozen_generated_types() -> Result {
    let case: review::ReviewCase = serde_json::from_str(include_str!(
        "../../../../../contracts/examples/review_case.valid.json"
    ))?;
    let resolution: agw_audit::contracts::review_resolution::ReviewResolution =
        serde_json::from_str(include_str!(
            "../../../../../contracts/examples/review_resolution.valid.json"
        ))?;
    assert_eq!(
        serde_json::to_value(case)?,
        serde_json::to_value(resolution.case)?
    );
    Ok(())
}
#[test]
fn oversized_or_deep_evidence_is_explicit_export_failure_and_never_releases_halt() -> Result {
    let config = RuntimeConfig::from_sources(Config::default(), None)?;
    config.reload_mode("strict")?;
    let gate = AvailabilityGate::new(true);
    for deep in [false, true] {
        let mut audit = draft()?;
        let mut value = serde_json::json!("x".repeat(if deep { 1 } else { review::MAX_BYTES }));
        if deep {
            for _ in 0..70 {
                value = serde_json::json!({"child": value});
            }
        }
        audit.metadata.insert("evidence".into(), value);
        let result: agw_guard::gated::Outcome<'_, ()> =
            Call::new(config.begin_call(None)?, &gate, 100_000).execute(
                Body::complete(b"{}"),
                &Engine::new(Some(Class::Shape)),
                audit,
                |_| panic!("halt"),
            );
        assert!(review::case(&result, &scope()?).is_err());
        assert_eq!(result.request.decision, Decision::Halt);
    }
    Ok(())
}
