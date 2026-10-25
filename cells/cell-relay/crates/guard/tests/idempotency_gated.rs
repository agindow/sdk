#[path = "support/idempotent_guard.rs"]
mod pipeline;
#[path = "support/idempotency.rs"]
mod support;
use agw_diff::{DriftClass as Class, Side};
use agw_guard::{
    config::{Config, RuntimeConfig},
    gated::{Body, Call},
    proxy::Headers,
    readiness::AvailabilityGate,
};
use agw_policy::Operation;
use pipeline::{Engine, ResponseOnly, draft};
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use support::{completed, request, store};
type Result = std::result::Result<(), Box<dyn std::error::Error>>;
static SOURCE: &[u8] = b"{ \"old\": 1e0, \"untouched\": 1.00 }";
static PATCHED: &[u8] = b"{ \"new\": 1e0, \"untouched\": 1.00 }";

#[test]
fn missing_write_key_is_400_before_inspection_even_while_warming_in_every_mode() -> Result {
    for mode in ["observe", "repair", "strict"] {
        for operation in [Operation::Write, Operation::Unknown] {
            let config = RuntimeConfig::from_sources(Config::default(), None)?;
            config.reload_mode(mode)?;
            let cache =
                store::<agw_guard::idempotency::Completion<()>>(2, Duration::from_secs(60))?;
            let engine = Engine::new(None);
            let gate = AvailabilityGate::new(false);
            let mut audit = draft()?;
            audit.operation = operation;
            let mut metadata = request("conn", None);
            metadata.operation = operation;
            let rejected = Call::new(config.begin_call(None)?, &gate, 100_000).execute_once(
                &cache,
                metadata,
                Body::complete(SOURCE),
                &engine,
                audit,
                |_, _| panic!("invalid write must not dispatch"),
            );
            assert_eq!(rejected.err().ok_or("expected 400")?.http_status(), 400);
            assert_eq!(engine.evaluations.load(Ordering::SeqCst), 0);
        }
    }
    Ok(())
}

#[test]
fn repaired_attempts_reuse_the_key_and_replay_the_original_outcome_without_repairing_again()
-> Result {
    let config = RuntimeConfig::from_sources(Config::default(), None)?;
    let cache = store(2, Duration::from_secs(60))?;
    let mut engine = Engine::new(Some(Class::Shape));
    engine.propose = true;
    let gate = AvailabilityGate::new(true);
    let mut commits = 0;
    let mut vendor = std::collections::HashMap::new();
    let first = completed(
        Call::new(config.begin_call(None)?, &gate, 100_000).execute_once(
            &cache,
            request("conn", Some("Original-Key")),
            Body::complete(SOURCE),
            &engine,
            draft()?,
            |sent, forwarding| {
                assert_eq!(sent, PATCHED);
                // Host-supplied retry after a lost acknowledgement. The fake
                // vendor commits each upstream key once, independently of cache.
                for _ in 0..2 {
                    let mut headers = Headers::new();
                    forwarding.apply_headers(&mut headers);
                    let key = headers.get("Idempotency-Key").unwrap_or("");
                    assert_eq!(key, "Original-Key");
                    vendor.entry(key.to_owned()).or_insert_with(|| {
                        commits += 1;
                        commits
                    });
                }
                (Box::new(AtomicUsize::new(commits)), Body::complete(SOURCE))
            },
        )?,
    )?;
    assert_eq!(commits, 1);
    assert_eq!(
        first
            .value
            .outcome
            .as_ref()
            .ok_or("missing upstream outcome")?
            .load(Ordering::SeqCst),
        1
    );
    assert_eq!(first.value.response_patch.as_deref(), Some(PATCHED));
    assert_eq!(
        first
            .value
            .audit
            .proof
            .idempotency_key
            .as_ref()
            .map(|key| key.as_str()),
        Some("Original-Key")
    );
    assert!(!first.value.audit.proof.write_integrity);
    assert!(first.value.audit.patch.as_ref().is_some_and(|p| p.applied));
    let second = completed(
        Call::new(config.begin_call(Some("strict"))?, &gate, 0).execute_once(
            &cache,
            request("conn", Some("Original-Key")),
            Body::complete(b"changed args"),
            &engine,
            draft()?,
            |_, _| panic!("replay cannot dispatch"),
        )?,
    )?;
    assert!(Arc::ptr_eq(&first.value, &second.value));
    assert_eq!(engine.evaluations.load(Ordering::SeqCst), 2);
    Ok(())
}

#[test]
fn blocked_request_is_terminal_even_after_mode_and_payload_change() -> Result {
    let config = RuntimeConfig::from_sources(Config::default(), None)?;
    let cache = store::<agw_guard::idempotency::Completion<()>>(2, Duration::from_secs(60))?;
    let gate = AvailabilityGate::new(true);
    let engine = Engine::new(Some(Class::Semantic));
    let first = completed(
        Call::new(config.begin_call(None)?, &gate, 100_000).execute_once(
            &cache,
            request("conn", Some("blocked")),
            Body::complete(SOURCE),
            &engine,
            draft()?,
            |_, _| panic!("semantic request halt must send nothing"),
        )?,
    )?;
    assert!(first.value.outcome.is_none());
    assert_eq!(first.value.audit.decision.to_string(), "blocked");
    assert!(
        first
            .value
            .feedback
            .as_ref()
            .is_some_and(|f| f.http_status == Some(409))
    );
    assert!(
        first
            .value
            .feedback
            .as_ref()
            .is_some_and(|feedback| feedback.payload.hint.contains("key is terminal")
                && feedback.payload.hint.contains("same outcome"))
    );
    config.reload_mode("observe")?;
    let clean = Engine::new(None);
    let replay = completed(
        Call::new(config.begin_call(None)?, &gate, 100_000).execute_once(
            &cache,
            request("conn", Some("blocked")),
            Body::complete(b"{}"),
            &clean,
            draft()?,
            |_, _| panic!("blocked key must stay terminal"),
        )?,
    )?;
    assert!(Arc::ptr_eq(&first.value, &replay.value));
    assert_eq!(clean.evaluations.load(Ordering::SeqCst), 0);
    Ok(())
}

#[test]
fn completed_response_halt_retains_and_replays_the_original_upstream_value() -> Result {
    let config = RuntimeConfig::from_sources(Config::default(), None)?;
    let cache = store(2, Duration::from_secs(60))?;
    let gate = AvailabilityGate::new(true);
    let engine = ResponseOnly(Engine::new(Some(Class::Semantic)));
    let first = completed(
        Call::new(config.begin_call(None)?, &gate, 100_000).execute_once(
            &cache,
            request("conn", Some("response-halt")),
            Body::complete(SOURCE),
            &engine,
            draft()?,
            |sent, _| {
                assert!(std::ptr::eq(sent, SOURCE));
                (Box::new(17), Body::complete(SOURCE))
            },
        )?,
    )?;
    assert_eq!(
        **first.value.outcome.as_ref().ok_or("completed write lost")?,
        17
    );
    assert_eq!(first.value.audit.decision.to_string(), "blocked");
    let replay = completed(Call::new(config.begin_call(None)?, &gate, 0).execute_once(
        &cache,
        request("conn", Some("response-halt")),
        Body::complete(SOURCE),
        &engine,
        draft()?,
        |_, _| panic!("must not repeat completed write"),
    )?)?;
    assert!(Arc::ptr_eq(&first.value, &replay.value));
    Ok(())
}

#[test]
fn warming_and_engine_faults_bypass_once_and_cache_the_original_result() -> Result {
    for (fault, panic) in [
        (None, false),
        (Some(Side::Request), false),
        (Some(Side::Response), false),
        (Some(Side::Request), true),
        (Some(Side::Response), true),
    ] {
        let config = RuntimeConfig::from_sources(Config::default(), None)?;
        let cache = store(2, Duration::from_secs(60))?;
        let gate = AvailabilityGate::new(fault.is_some());
        let mut engine = Engine::new(None);
        engine.fault = fault;
        engine.panic = panic;
        let first = completed(
            Call::new(config.begin_call(None)?, &gate, 100_000).execute_once(
                &cache,
                request("conn", Some("key")),
                Body::complete(SOURCE),
                &engine,
                draft()?,
                |sent, _| {
                    assert!(std::ptr::eq(sent, SOURCE));
                    (Box::new(17), Body::complete(SOURCE))
                },
            )?,
        )?;
        assert_eq!(first.value.bypasses, 1);
        assert_eq!(first.value.audit.decision.to_string(), "bypassed");
        gate.set_ready(true);
        let replay = completed(
            Call::new(config.begin_call(None)?, &gate, 100_000).execute_once(
                &cache,
                request("conn", Some("key")),
                Body::complete(SOURCE),
                &engine,
                draft()?,
                |_, _| panic!("bypass replay must not write again"),
            )?,
        )?;
        assert!(Arc::ptr_eq(&first.value, &replay.value));
    }
    Ok(())
}

#[test]
fn read_calls_never_use_write_capacity_or_claim_terminal_keys() -> Result {
    let config = RuntimeConfig::from_sources(Config::default(), None)?;
    let cache = store::<agw_guard::idempotency::Completion<usize>>(1, Duration::from_secs(60))?;
    let gate = AvailabilityGate::new(true);
    let blocked = Engine::new(Some(Class::Semantic));
    completed(
        Call::new(config.begin_call(None)?, &gate, 100_000).execute_once(
            &cache,
            request("conn", Some("blocked")),
            Body::complete(SOURCE),
            &blocked,
            draft()?,
            |_, _| panic!("blocked write must send nothing"),
        )?,
    )?;
    for value in [17, 18] {
        let mut metadata = request("conn", Some("blocked"));
        metadata.operation = Operation::Read;
        let mut audit = draft()?;
        audit.operation = Operation::Read;
        let result = completed(
            Call::new(config.begin_call(None)?, &gate, 100_000).execute_once(
                &cache,
                metadata,
                Body::complete(SOURCE),
                &blocked,
                audit,
                |_, _| (value, Body::complete(SOURCE)),
            )?,
        )?;
        assert_eq!(result.value.outcome, Some(value));
        assert!(!result.replayed);
        assert_eq!(
            result.value.audit.metadata["agindow_idempotency"]["retention"],
            "not_retained"
        );
        assert!(
            result
                .value
                .feedback
                .as_ref()
                .is_some_and(|feedback| !feedback.payload.hint.contains("key is terminal"))
        );
    }
    Ok(())
}

#[test]
fn operation_mismatch_cannot_bypass_write_key_admission() -> Result {
    let config = RuntimeConfig::from_sources(Config::default(), None)?;
    let cache = store::<agw_guard::idempotency::Completion<()>>(1, Duration::from_secs(60))?;
    let gate = AvailabilityGate::new(true);
    let engine = Engine::new(None);
    let mut metadata = request("conn", None);
    metadata.operation = Operation::Read;
    let error = Call::new(config.begin_call(None)?, &gate, 100_000)
        .execute_once(
            &cache,
            metadata,
            Body::complete(SOURCE),
            &engine,
            draft()?,
            |_, _| panic!("mismatched envelope must not dispatch"),
        )
        .err()
        .ok_or("expected mismatch rejection")?;
    assert_eq!(error.http_status(), 400);
    assert_eq!(engine.evaluations.load(Ordering::SeqCst), 0);
    Ok(())
}
