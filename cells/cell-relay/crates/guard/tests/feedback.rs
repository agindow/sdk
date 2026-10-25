#[path = "support/gated.rs"]
mod support;
use agw_diff::DriftClass;
use agw_guard::{
    config::{Config, RuntimeConfig},
    feedback::{ErrorCode, Feedback, FeedbackCode, Payload},
    gated::{Body, Call},
    readiness::AvailabilityGate,
};
use agw_policy::{Mode, Operation};
use support::{Engine, draft};
type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn taxonomy_is_exactly_contract_seven() -> TestResult {
    let expected = [
        (ErrorCode::InvalidRequest, "invalid_request", 400),
        (ErrorCode::Unauthorized, "unauthorized", 401),
        (ErrorCode::PlanLimit, "plan_limit", 403),
        (ErrorCode::DriftBlocked, "drift_blocked", 409),
        (ErrorCode::DriftHalted, "drift_halted", 409),
        (ErrorCode::ContractViolation, "contract_violation", 422),
        (ErrorCode::RateLimited, "rate_limited", 429),
        (ErrorCode::Internal, "internal", 500),
    ];
    for (code, name, status) in expected {
        assert_eq!(serde_json::to_value(code)?, name);
        assert_eq!(code.http_status(), status);
    }
    assert!(serde_json::from_str::<ErrorCode>("\"drift_passthrough\"").is_err());
    Ok(())
}

#[test]
fn outcomes_map_to_feedback_without_altering_dispatch_or_bytes() -> TestResult {
    for mode in [Mode::Observe, Mode::Repair, Mode::Strict] {
        for operation in [Operation::Read, Operation::Write, Operation::Unknown] {
            for class in [
                None,
                Some(DriftClass::Shape),
                Some(DriftClass::Semantic),
                Some(DriftClass::Unknown),
            ] {
                for ready in [false, true] {
                    let runtime = RuntimeConfig::from_sources(
                        Config {
                            mode: Some(mode.to_string()),
                            ..Config::default()
                        },
                        None,
                    )?;
                    let gate = AvailabilityGate::new(ready);
                    let engine = Engine::new(class);
                    let mut audit = draft()?;
                    audit.operation = operation;
                    let mut dispatched = 0;
                    let result = Call::new(runtime.begin_call(None)?, &gate, 100_000).execute(
                        Body::complete(b"{\"old\":1}"),
                        &engine,
                        audit,
                        |bytes| {
                            dispatched += 1;
                            assert_eq!(bytes, b"{\"old\":1}");
                            (42, Body::complete(b"{\"old\":1}"))
                        },
                    );
                    let feedback = Feedback::from_outcome(&result);
                    if !ready || mode == Mode::Observe || class.is_none() {
                        assert!(feedback.is_none());
                        assert_eq!(result.outcome, Some(42));
                        continue;
                    }
                    let feedback = feedback.ok_or("missing feedback")?;
                    let halt = mode == Mode::Strict
                        || (operation != Operation::Read && class != Some(DriftClass::Shape));
                    assert_eq!(dispatched, usize::from(!halt));
                    assert_eq!(feedback.http_status, halt.then_some(409));
                    assert_eq!(
                        feedback.payload.error,
                        if mode == Mode::Strict {
                            FeedbackCode::DriftHalted
                        } else if halt {
                            FeedbackCode::DriftBlocked
                        } else {
                            FeedbackCode::DriftPassthrough
                        }
                    );
                    assert_eq!(feedback.payload.drift_id, result.audit.id.to_string());
                    let json = serde_json::to_value(&feedback.payload)?;
                    assert_eq!(json.as_object().ok_or("object")?.len(), 6);
                    assert_eq!(serde_json::from_value::<Payload>(json)?, feedback.payload);
                    assert!(
                        feedback
                            .payload
                            .to_string()
                            .contains(&feedback.payload.hint)
                    );
                    if class == Some(DriftClass::Shape) {
                        assert!(feedback.payload.hint.contains("`old` → `new`"));
                    } else {
                        assert!(feedback.payload.hint.contains("human review"));
                        assert!(!feedback.payload.hint.contains("Re-issue"));
                    }
                }
            }
        }
    }
    Ok(())
}
