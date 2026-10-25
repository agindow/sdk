#[path = "support/gated.rs"]
mod support;
use agw_diff::{DriftClass as Class, Kind, Reason, Side};
use agw_guard::{
    availability::{Budget, EngineFault},
    config::{Config, RuntimeConfig},
    feedback::{Feedback, FeedbackCode},
    gated::{Body, Call, Evidence, Pipeline},
    readiness::AvailabilityGate,
};
use std::sync::atomic::Ordering;
use support::{Engine, draft};
type TestResult = Result<(), Box<dyn std::error::Error>>;

struct Adapter {
    engine: Engine,
    action: u8,
}
impl Pipeline for Adapter {
    fn detect(
        &self,
        side: Side,
        bytes: &[u8],
        budget: &mut Budget,
    ) -> Result<Evidence, EngineFault> {
        let mut e = self.engine.detect(side, bytes, budget)?;
        if self.action == 1 && side == Side::Request {
            e.report.changes.clear();
        }
        if self.action == 2 {
            let mut c = e.report.changes[0].clone();
            c.class = Class::Semantic;
            c.path = "amount".into();
            c.kind = Kind::UnitsChanged;
            c.reason = Reason::UnitsChanged;
            e.report.changes.push(c);
        }
        if self.action == 3 {
            e.report.changes[0].reason = Reason::AmbiguousIdentity;
        }
        if self.action == 4 {
            e.report.changes[0].path = "source".into();
            e.report.changes[0].to_path = Some("payment_method".into());
        }
        Ok(e)
    }
    fn verify(
        &self,
        side: Side,
        bytes: &[u8],
        findings: &[usize],
        budget: &mut Budget,
    ) -> Result<bool, EngineFault> {
        self.engine.verify(side, bytes, findings, budget)
    }
}
fn runtime(mode: &str) -> Result<RuntimeConfig, Box<dyn std::error::Error>> {
    Ok(RuntimeConfig::from_sources(
        Config {
            mode: Some(mode.into()),
            ..Config::default()
        },
        None,
    )?)
}

#[test]
fn response_halt_keeps_completed_outcome_and_warns_against_replay() -> TestResult {
    let runtime = runtime("repair")?;
    let gate = AvailabilityGate::new(true);
    let adapter = Adapter {
        engine: Engine::new(Some(Class::Semantic)),
        action: 1,
    };
    let result = Call::new(runtime.begin_call(None)?, &gate, 100_000).execute(
        Body::complete(b"{}"),
        &adapter,
        draft()?,
        |_| (42, Body::complete(b"{}")),
    );
    let feedback = Feedback::from_outcome(&result).ok_or("feedback")?;
    assert_eq!(result.outcome, Some(42));
    assert_eq!(
        result.response.as_ref().ok_or("response")?.body.as_ref(),
        b"{}"
    );
    assert_eq!(feedback.payload.error, FeedbackCode::DriftBlocked);
    assert!(feedback.payload.hint.contains("already dispatched"));
    assert!(feedback.payload.hint.contains("Do not blindly replay"));
    Ok(())
}

#[test]
fn mixed_drift_cannot_offer_shape_rename_as_semantic_fix() -> TestResult {
    let runtime = runtime("repair")?;
    let gate = AvailabilityGate::new(true);
    let adapter = Adapter {
        engine: Engine::new(Some(Class::Shape)),
        action: 2,
    };
    let result = Call::new(runtime.begin_call(None)?, &gate, 100_000).execute(
        Body::complete(b"{}"),
        &adapter,
        draft()?,
        |_| (42, Body::complete(b"{}")),
    );
    let p = Feedback::from_outcome(&result).ok_or("feedback")?.payload;
    assert_eq!(result.outcome, None);
    assert_eq!(p.drift_class.to_string(), "semantic");
    assert!(p.hint.contains("`amount`"));
    assert!(p.hint.contains("human review"));
    assert!(!p.hint.contains('→'));
    Ok(())
}

#[test]
fn confirmed_rename_is_actionable_and_audit_id_is_one_encoded_segment() -> TestResult {
    let runtime = runtime("strict")?;
    let gate = AvailabilityGate::new(true);
    let adapter = Adapter {
        engine: Engine::new(Some(Class::Shape)),
        action: 4,
    };
    let mut audit = draft()?;
    audit.id = "aud_x/?#%é".try_into()?;
    let result = Call::new(runtime.begin_call(None)?, &gate, 100_000).execute(
        Body::complete(b"{}"),
        &adapter,
        audit,
        |_| (42, Body::complete(b"{}")),
    );
    let p = Feedback::from_outcome(&result).ok_or("feedback")?.payload;
    assert!(p.hint.contains("`source` → `payment_method`"));
    assert!(p.hint.contains("not dispatched"));
    assert!(!p.hint.contains("2026"));
    assert_eq!(
        p.audit,
        "https://api.agindow.com/v1/audit/aud_x%2F%3F%23%25%C3%A9"
    );
    Ok(())
}

#[test]
fn unconfirmed_identity_requests_schema_instead_of_inventing_expected_form() -> TestResult {
    let runtime = runtime("strict")?;
    let gate = AvailabilityGate::new(true);
    let adapter = Adapter {
        engine: Engine::new(Some(Class::Shape)),
        action: 3,
    };
    let result = Call::new(runtime.begin_call(None)?, &gate, 100_000).execute(
        Body::complete(b"{}"),
        &adapter,
        draft()?,
        |_| (42, Body::complete(b"{}")),
    );
    let hint = Feedback::from_outcome(&result)
        .ok_or("feedback")?
        .payload
        .hint;
    assert!(hint.contains("expected form is unavailable"));
    assert!(hint.contains("`old`"));
    assert!(!hint.contains('→'));
    Ok(())
}

#[test]
fn verified_repair_and_engine_fault_never_become_policy_errors() -> TestResult {
    for fault in [None, Some(Side::Request), Some(Side::Response)] {
        let runtime = runtime("repair")?;
        let gate = AvailabilityGate::new(true);
        let mut engine = Engine::new(Some(Class::Shape));
        engine.propose = true;
        engine.fault = fault;
        let result = Call::new(runtime.begin_call(None)?, &gate, 100_000).execute(
            Body::complete(b"{\"old\":1}"),
            &engine,
            draft()?,
            |_| (42, Body::complete(b"{\"old\":1}")),
        );
        assert!(Feedback::from_outcome(&result).is_none());
        assert_eq!(result.outcome, Some(42));
        assert_eq!(result.bypasses, u8::from(fault.is_some()));
        if fault.is_none() {
            assert_eq!(engine.verifications.load(Ordering::SeqCst), 2);
        }
    }
    Ok(())
}

#[test]
fn malformed_write_is_additive_passthrough_feedback() -> TestResult {
    let runtime = runtime("repair")?;
    let gate = AvailabilityGate::new(true);
    let engine = Engine::new(None);
    let result = Call::new(runtime.begin_call(None)?, &gate, 100_000).execute(
        Body::complete(b"{"),
        &engine,
        draft()?,
        |bytes| {
            assert_eq!(bytes, b"{");
            (422, Body::complete(b"{}"))
        },
    );
    let feedback = Feedback::from_outcome(&result).ok_or("feedback")?;
    assert_eq!(feedback.http_status, None);
    assert_eq!(feedback.payload.error, FeedbackCode::DriftPassthrough);
    assert_eq!(result.outcome, Some(422));
    assert!(feedback.payload.hint.contains("complete valid JSON"));
    Ok(())
}

#[test]
fn blocked_then_corrected_call_records_verified_task_recovery() -> TestResult {
    use agw_guard::feedback::recovery::{RetryOutcome, Tracker};
    let runtime = runtime("strict")?;
    let gate = AvailabilityGate::new(true);
    let adapter = Adapter {
        engine: Engine::new(Some(Class::Shape)),
        action: 4,
    };
    let blocked = Call::new(runtime.begin_call(None)?, &gate, 100_000).execute(
        Body::complete(b"{\"source\":1}"),
        &adapter,
        draft()?,
        |_| (false, Body::complete(b"{}")),
    );
    let feedback = Feedback::from_outcome(&blocked).ok_or("feedback")?;
    assert_eq!(blocked.outcome, None);
    let mut tracker = Tracker::new(10)?;
    tracker.blocked(&feedback.payload.tool, &feedback.payload.drift_id)?;
    // The host/agent explicitly corrects the call using authoritative evidence.
    let corrected = b"{\"payment_method\":1}";
    let clean = Engine::new(None);
    let retry = Call::new(runtime.begin_call(None)?, &gate, 100_000).execute(
        Body::complete(corrected),
        &clean,
        draft()?,
        |bytes| {
            assert_eq!(bytes, corrected);
            (true, Body::complete(b"{}"))
        },
    );
    assert_eq!(retry.outcome, Some(true));
    assert!(Feedback::from_outcome(&retry).is_none());
    // Test host verifies the task result separately from the HTTP/policy status.
    tracker.retry(
        &feedback.payload.tool,
        &feedback.payload.drift_id,
        RetryOutcome::VerifiedTaskSuccess,
    )?;
    assert_eq!(tracker.snapshot().recovery_rate(), Some(1.0));
    assert_eq!(tracker.snapshot().recovered, 1);
    Ok(())
}

#[test]
fn oversized_rename_target_never_becomes_a_truncated_correction() -> TestResult {
    let runtime = runtime("strict")?;
    let gate = AvailabilityGate::new(true);
    let engine = Engine::new(Some(Class::Shape));
    let mut result = Call::new(runtime.begin_call(None)?, &gate, 100_000).execute(
        Body::complete(b"{}"),
        &engine,
        draft()?,
        |_| (42, Body::complete(b"{}")),
    );
    result.request.report.changes[0].to_path = Some("x".repeat(1024));
    let hint = Feedback::from_outcome(&result)
        .ok_or("feedback")?
        .payload
        .hint;
    assert!(!hint.contains("Update the call to use"));
    assert!(!hint.contains('→'));
    assert!(hint.len() < 1024);
    Ok(())
}
