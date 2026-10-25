#[path = "support/statistical.rs"]
mod support;
use agw_diff::{DriftClass, Evidence, Node, Side, Snapshot};
use agw_guard::{
    config::{Config, RuntimeConfig},
    statistical::{Decision, Engine, Operation, Plan},
};
use agw_stats::router::{Admission, Outbox};
use support::{body, context, field, identity};
type Result = std::result::Result<(), Box<dyn std::error::Error>>;

fn unknown_plan() -> std::result::Result<Plan, Box<dyn std::error::Error>> {
    let mut plan = Plan::new(identity(), vec![])?;
    let shape = Snapshot {
        node: Node::object([]),
        evidence: Evidence::Learned,
    };
    plan.request_shape = Some(shape.clone());
    plan.response_shape = Some(shape);
    plan.investigation_ceiling = 5;
    Ok(plan)
}
#[test]
fn mode_operation_side_matrix_preserves_original_bytes_and_attempt_counts() -> Result {
    for mode in ["observe", "repair", "strict"] {
        for operation in [Operation::Read, Operation::Write, Operation::Unknown] {
            for side in [Side::Request, Side::Response] {
                let engine =
                    Engine::statistical(Plan::new(identity(), vec![field(side, "1", "100")?])?);
                let bytes = b"{ \"value\" : 100 }";
                let count = std::cell::Cell::new(0);
                let output = engine
                    .begin_call(context(mode)?, operation, "audit", 100_000_000, 0)
                    .execute(body(bytes), None, || {
                        count.set(count.get() + 1);
                        (bytes.as_slice(), body(bytes))
                    });
                let halt = mode == "strict" || (mode == "repair" && operation != Operation::Read);
                assert_eq!(count.get(), usize::from(!(halt && side == Side::Request)));
                if let Some(bytes_out) = output.outcome {
                    assert!(std::ptr::eq(bytes_out, bytes.as_slice()));
                }
                let phase = if side == Side::Request {
                    &output.request
                } else {
                    output.response.as_ref().ok_or("response")?
                };
                assert_eq!(
                    phase.decision,
                    if mode == "observe" {
                        Decision::Pass
                    } else if halt {
                        Decision::Halt
                    } else {
                        Decision::Degraded
                    }
                );
                assert_eq!(phase.report.class(), Some(DriftClass::Semantic));
            }
        }
    }
    Ok(())
}
#[test]
fn unknown_investigation_shares_one_tenant_scoped_budget_across_phases() -> Result {
    for (capacity, limit, admission) in [
        (0, 10, Admission::Disabled),
        (1, 0, Admission::BudgetExhausted),
        (2, 10, Admission::Enqueued),
    ] {
        let engine = Engine::statistical(unknown_plan()?);
        let mut outbox = Outbox::new(capacity)?;
        let output = engine
            .begin_call(
                context("repair")?,
                Operation::Read,
                "audit",
                100_000_000,
                limit,
            )
            .execute(body(b"{\"extra\":1}"), Some(&mut outbox), || {
                ("original", body(b"{\"extra\":2}"))
            });
        assert_eq!(output.request.report.class(), Some(DriftClass::Unknown));
        assert_eq!(output.request.decision, Decision::Degraded);
        assert_eq!(output.request.investigation, admission);
        let response = output.response.ok_or("response")?;
        assert_eq!(response.decision, Decision::Degraded);
        assert_eq!(
            response.investigation,
            if admission == Admission::Enqueued {
                Admission::AlreadySubmitted
            } else {
                admission
            }
        );
        assert_eq!(outbox.len(), usize::from(admission == Admission::Enqueued));
        assert_eq!(
            output.reserved,
            if admission == Admission::Enqueued {
                5
            } else {
                0
            }
        );
        if let Some(job) = outbox.pop() {
            assert_eq!(job.audit_ref(), "6:tenant:audit");
        }
    }
    Ok(())
}
#[test]
fn full_outbox_never_changes_policy_and_known_findings_never_enqueue() -> Result {
    let engine = Engine::statistical(unknown_plan()?);
    let mut outbox = Outbox::new(1)?;
    for admission in [Admission::Enqueued, Admission::Full] {
        let output = engine
            .begin_call(
                context("strict")?,
                Operation::Write,
                "audit",
                100_000_000,
                10,
            )
            .execute::<(), _>(body(b"{\"extra\":1}"), Some(&mut outbox), || {
                panic!("request must halt")
            });
        assert_eq!(output.request.investigation, admission);
        assert_eq!(output.request.decision, Decision::Halt);
    }
    let engine = Engine::statistical(Plan::new(
        identity(),
        vec![field(Side::Request, "1", "100")?],
    )?);
    let output = engine
        .begin_call(
            context("repair")?,
            Operation::Write,
            "audit",
            100_000_000,
            10,
        )
        .execute::<(), _>(body(b"{\"value\":100}"), Some(&mut outbox), || {
            panic!("semantic block")
        });
    assert_eq!(output.request.investigation, Admission::NotNeeded);
    assert_eq!(outbox.len(), 1);
    Ok(())
}
#[test]
fn reloads_preserve_the_call_mode_and_baseline_generations() -> Result {
    let runtime = RuntimeConfig::from_sources(
        Config {
            mode: Some("observe".into()),
            ..Config::default()
        },
        None,
    )?;
    let mut engine = Engine::statistical(Plan::new(
        identity(),
        vec![field(Side::Response, "1", "100")?],
    )?);
    let call = engine.begin_call(
        runtime.begin_call(None)?,
        Operation::Write,
        "audit",
        100_000_000,
        0,
    );
    let output = call.execute(body(b"{}"), None, || {
        runtime
            .reload_mode("strict")
            .unwrap_or_else(|e| panic!("{e}"));
        let replacement = Plan::new(identity(), vec![]).unwrap_or_else(|e| panic!("{e}"));
        engine.replace(replacement);
        ("original", body(b"{\"value\":100}"))
    });
    assert_eq!(output.mode, agw_guard::config::Mode::Observe);
    let response = output.response.ok_or("response")?;
    assert_eq!(response.decision, Decision::Pass);
    assert_eq!(response.records[0].generation, "v1");
    let next = engine
        .begin_call(
            runtime.begin_call(None)?,
            Operation::Write,
            "next",
            100_000_000,
            0,
        )
        .execute(body(b"{}"), None, || ("next", body(b"{}")));
    assert_eq!(next.mode, agw_guard::config::Mode::Strict);
    assert!(next.response.ok_or("response")?.records.is_empty());
    Ok(())
}
#[test]
fn semantic_findings_survive_shape_and_unknown_findings() -> Result {
    let mut plan = Plan::new(identity(), vec![field(Side::Request, "1", "100")?])?;
    let mut node = Node::object([]);
    node.properties.insert(
        "old".into(),
        Node::scalar(agw_inspector::inference::JsonType::String),
    );
    let before = Snapshot {
        node,
        evidence: Evidence::Authoritative,
    };
    let after = Snapshot {
        node: Node::object([]),
        evidence: Evidence::Authoritative,
    };
    let shape = agw_diff::compare(Side::Request, &before, &after);
    plan.request_shape = Some(after);
    let mut request = body(b"{\"value\":100}");
    request.findings = Some(&shape);
    let engine = Engine::statistical(plan);
    let output = engine
        .begin_call(
            context("repair")?,
            Operation::Write,
            "audit",
            100_000_000,
            0,
        )
        .execute::<(), _>(request, None, || panic!("semantic finding must block"));
    assert!(
        output
            .request
            .report
            .changes
            .iter()
            .any(|c| c.class == DriftClass::Shape)
    );
    assert!(
        output
            .request
            .report
            .changes
            .iter()
            .any(|c| c.class == DriftClass::Semantic)
    );
    assert_eq!(output.request.decision, Decision::Halt);
    Ok(())
}
