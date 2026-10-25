#[path = "support/statistical.rs"]
mod support;
use agw_diff::{Evidence, Node, Side, Snapshot};
use agw_guard::{
    config::{Config, RuntimeConfig},
    statistical::{Decision, Engine, Operation, Plan},
};
use support::{body, field, identity};
type Result = std::result::Result<(), Box<dyn std::error::Error>>;

#[test]
fn statistical_unknown_obeys_opt_out_on_both_sides() -> Result {
    for mode in ["repair", "strict"] {
        for closed in [true, false] {
            let config = RuntimeConfig::from_sources(Config::default(), None)?;
            config.reload_mode(mode)?;
            config.reload_fail_closed(closed, "owner", chrono::DateTime::UNIX_EPOCH)?;
            let mut plan = Plan::new(identity(), vec![])?;
            let learned = Snapshot {
                node: Node::object([]),
                evidence: Evidence::Learned,
            };
            plan.request_shape = Some(learned.clone());
            plan.response_shape = Some(learned);
            let engine = Engine::statistical(plan);
            let bytes = b"{\"extra\":1}";
            let result = engine
                .begin_call(
                    config.begin_call(None)?,
                    Operation::Write,
                    "audit",
                    100_000_000,
                    0,
                )
                .execute(body(bytes), None, || (bytes.as_slice(), body(bytes)));
            let halted = closed || mode == "strict";
            assert_eq!(result.request.decision == Decision::Halt, halted);
            assert_eq!(result.dispatched, !halted);
            if !halted {
                assert_eq!(
                    result.response.ok_or("response")?.decision,
                    Decision::Degraded
                );
                assert!(std::ptr::eq(
                    result.outcome.ok_or("outcome")?,
                    bytes.as_slice()
                ));
            }
        }
    }
    Ok(())
}

#[test]
fn opt_out_never_overrides_mixed_semantic_or_completed_response_halts() -> Result {
    for side in [Side::Request, Side::Response] {
        let config = RuntimeConfig::from_sources(Config::default(), None)?;
        config.reload_fail_closed(false, "owner", chrono::DateTime::UNIX_EPOCH)?;
        let mut plan = Plan::new(identity(), vec![field(side, "1", "100")?])?;
        let learned = Snapshot {
            node: Node::object([]),
            evidence: Evidence::Learned,
        };
        match side {
            Side::Request => plan.request_shape = Some(learned),
            Side::Response => plan.response_shape = Some(learned),
        }
        let engine = Engine::statistical(plan);
        let result = engine
            .begin_call(
                config.begin_call(None)?,
                Operation::Write,
                "audit",
                100_000_000,
                0,
            )
            .execute(body(b"{\"value\":100}"), None, || {
                (17, body(b"{\"value\":100}"))
            });
        assert_eq!(result.dispatched, side == Side::Response);
        let phase = match side {
            Side::Request => &result.request,
            Side::Response => result.response.as_ref().ok_or("response")?,
        };
        assert_eq!(phase.decision, Decision::Halt);
        if side == Side::Response {
            assert_eq!(result.outcome, Some(17));
        }
    }
    Ok(())
}
