#[path = "support/gated.rs"]
mod support;
use agw_diff::{DriftClass, Side};
use agw_guard::{
    availability::{Budget, EngineFault},
    config::{Config, RuntimeConfig},
    gated::{Body, Call, Evidence, Pipeline},
    readiness::AvailabilityGate,
};
use agw_policy::Operation;
use support::{Engine, draft};
type TestResult = Result<(), Box<dyn std::error::Error>>;
use std::sync::atomic::Ordering;

struct Adapter {
    engine: Engine,
    action: u8,
}
impl Pipeline for Adapter {
    fn detect(&self, side: Side, bytes: &[u8], work: &mut Budget) -> Result<Evidence, EngineFault> {
        let mut evidence = self.engine.detect(side, bytes, work)?;
        match self.action {
            1 => {
                let mut c = evidence.report.changes[0].clone();
                c.class = DriftClass::Semantic;
                c.path = "other".into();
                evidence.report.changes.push(c);
            }
            2 => {
                evidence.report.changes[0].side = Side::Response;
            }
            3 => {
                evidence.report.skipped = Some(agw_diff::Skip::Incomplete);
            }
            4 => {
                let _ = work.charge(usize::MAX);
            }
            _ => {}
        }
        Ok(evidence)
    }
    fn verify(
        &self,
        side: Side,
        bytes: &[u8],
        findings: &[usize],
        work: &mut Budget,
    ) -> Result<bool, EngineFault> {
        match self.action {
            5 => Err(EngineFault::Internal),
            6 => panic!("verifier panic"),
            _ => self.engine.verify(side, bytes, findings, work),
        }
    }
}
fn runtime(mode: &str) -> Result<RuntimeConfig, Box<dyn std::error::Error>> {
    RuntimeConfig::from_sources(
        Config {
            mode: Some(mode.into()),
            ..Config::default()
        },
        None,
    )
    .map_err(Into::into)
}
#[test]
fn blocked_mixed_findings_do_not_claim_an_unsent_patch() -> TestResult {
    let runtime = runtime("repair")?;
    let mut engine = Engine::new(Some(DriftClass::Shape));
    engine.propose = true;
    let pipeline = Adapter { engine, action: 1 };
    let gate = AvailabilityGate::new(true);
    let result = Call::new(runtime.begin_call(None)?, &gate, 100_000).execute(
        Body::complete(b"{\"old\":1}"),
        &pipeline,
        draft()?,
        |_| -> ((), Body<'_>) { panic!("blocked request dispatched") },
    );
    assert!(result.outcome.is_none());
    assert_eq!(result.audit.decision.to_string(), "blocked");
    assert_eq!(
        result
            .audit
            .drift
            .ok_or("expected outcome")?
            .class
            .to_string(),
        "semantic"
    );
    assert!(result.audit.patch.is_none());
    assert!(matches!(result.request.body, std::borrow::Cow::Borrowed(_)));
    Ok(())
}
#[test]
fn invalid_evidence_and_verifier_faults_bypass_before_policy() -> TestResult {
    for action in 2..=6 {
        let mut engine = Engine::new(Some(DriftClass::Shape));
        engine.propose = true;
        let pipeline = Adapter { engine, action };
        let runtime = runtime("strict")?;
        let gate = AvailabilityGate::new(true);
        let original = b"{\"old\":1}";
        let result = Call::new(runtime.begin_call(None)?, &gate, 100_000).execute(
            Body::complete(original),
            &pipeline,
            draft()?,
            |sent| {
                assert!(std::ptr::eq(sent, original));
                (5, Body::complete(original))
            },
        );
        assert_eq!(result.outcome, Some(5));
        assert_eq!(result.bypasses, 1);
        assert!(result.response.is_none());
        assert_eq!(result.audit.decision.to_string(), "bypassed");
        assert!(result.audit.drift.is_none());
        assert!(result.audit.patch.is_none());
        assert!(!result.audit.proof.write_integrity);
    }
    Ok(())
}
#[test]
fn incomplete_duplicate_and_exhausted_captures_are_not_vendor_drift() -> TestResult {
    for (bytes, truncated, budget) in [
        (&b"{"[..], true, 100_000),
        (&b"{\"old\":1,\"old\":2}"[..], false, 100_000),
        (&b"{}"[..], false, 0),
    ] {
        let runtime = runtime("strict")?;
        let gate = AvailabilityGate::new(true);
        let engine = Engine::new(Some(DriftClass::Semantic));
        let result = Call::new(runtime.begin_call(None)?, &gate, budget).execute(
            Body { bytes, truncated },
            &engine,
            draft()?,
            |_| (5, Body::complete(b"{}")),
        );
        assert_eq!(result.outcome, Some(5));
        assert_eq!(result.bypasses, 1);
        assert!(result.audit.drift.is_none());
    }
    Ok(())
}
#[test]
fn complete_malformed_bodies_retain_the_special_policy() -> TestResult {
    for mode in ["observe", "repair", "strict"] {
        for op in [Operation::Read, Operation::Write, Operation::Unknown] {
            let runtime = runtime(mode)?;
            let gate = AvailabilityGate::new(true);
            let engine = Engine::new(None);
            let mut audit = draft()?;
            audit.operation = op;
            let result = Call::new(runtime.begin_call(None)?, &gate, 100_000).execute(
                Body::complete(b"{"),
                &engine,
                audit,
                |_| (5, Body::complete(b"{")),
            );
            let expected = match (mode, op) {
                ("observe", _) => "ok",
                ("strict", _) => "blocked",
                (_, Operation::Read) => "degraded",
                _ => "bypassed",
            };
            assert_eq!(result.audit.decision.to_string(), expected);
            assert_eq!(result.outcome.is_some(), mode != "strict");
            assert_eq!(result.bypasses, 0);
            assert_eq!(
                result
                    .audit
                    .drift
                    .ok_or("expected outcome")?
                    .class
                    .to_string(),
                "shape"
            );
            let patch = result.audit.patch.ok_or("malformed byte identity")?;
            assert!(!patch.applied && patch.byte_identical);
            assert_eq!(result.audit.confidence, Some(0.0));
            assert_eq!(result.headers.get("X-Agindow-Repair"), Some("skipped"));
            assert_eq!(engine.evaluations.load(Ordering::SeqCst), 0);
        }
    }
    Ok(())
}
#[test]
fn response_halt_retains_original_error_and_does_not_claim_prevention() -> TestResult {
    struct ResponseOnly(Engine);
    impl Pipeline for ResponseOnly {
        fn detect(
            &self,
            side: Side,
            bytes: &[u8],
            work: &mut Budget,
        ) -> Result<Evidence, EngineFault> {
            if side == Side::Request {
                Ok(Evidence::default())
            } else {
                self.0.detect(side, bytes, work)
            }
        }
        fn verify(
            &self,
            _: Side,
            _: &[u8],
            _: &[usize],
            _: &mut Budget,
        ) -> Result<bool, EngineFault> {
            unreachable!()
        }
    }
    let runtime = runtime("repair")?;
    let gate = AvailabilityGate::new(true);
    let pipeline = ResponseOnly(Engine::new(Some(DriftClass::Semantic)));
    let err = Box::new("vendor failure");
    let ptr = &*err as *const &str;
    let result = Call::new(runtime.begin_call(None)?, &gate, 100_000).execute(
        Body::complete(b"{}"),
        &pipeline,
        draft()?,
        |_| (Err::<(), _>(err), Body::complete(b"{}")),
    );
    assert_eq!(result.audit.decision.to_string(), "blocked");
    assert_eq!(
        &*result
            .outcome
            .ok_or("expected outcome")?
            .err()
            .ok_or("vendor error")? as *const &str,
        ptr
    );
    assert_eq!(
        result
            .audit
            .drift
            .ok_or("expected outcome")?
            .side
            .to_string(),
        "response"
    );
    Ok(())
}
#[test]
fn repeated_faults_open_the_shared_breaker_without_evaluating_again() -> TestResult {
    let runtime = runtime("strict")?;
    let gate = AvailabilityGate::new(true);
    let mut engine = Engine::new(Some(DriftClass::Semantic));
    engine.fault = Some(Side::Request);
    for _ in 0..4 {
        let result = Call::new(runtime.begin_call(None)?, &gate, 100_000).execute(
            Body::complete(b"{}"),
            &engine,
            draft()?,
            |_| (5, Body::complete(b"{}")),
        );
        assert_eq!(result.bypasses, 1);
        assert_eq!(result.outcome, Some(5));
    }
    assert_eq!(engine.evaluations.load(Ordering::SeqCst), 3);
    Ok(())
}
