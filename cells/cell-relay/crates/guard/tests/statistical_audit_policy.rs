//! TI-47 audit policy precedence across malformed and semantic evidence.
#[path = "support/audit.rs"]
mod audit;
#[path = "support/statistical.rs"]
mod support;
use agw_audit::contracts::audit_artifact::{
    AuditArtifactDecision as Verdict, DriftClass, DriftSide,
};
use agw_diff::Side;
use agw_guard::statistical::{Decision, Engine, Operation, Plan};
use support::{body, context, field, identity};
type Result = std::result::Result<(), Box<dyn std::error::Error>>;

#[test]
fn malformed_request_cannot_hide_a_semantic_response_halt() -> Result {
    let engine = Engine::statistical(Plan::new(
        identity(),
        vec![field(Side::Response, "1", "100")?],
    )?);
    let output = engine
        .begin_call(
            context("repair")?,
            Operation::Write,
            "audit",
            1_000_000_000,
            0,
        )
        .execute(body(b"{"), None, || ("original", body(br#"{"value":100}"#)));
    assert_eq!(output.request.decision, Decision::Pass);
    assert_eq!(
        output.response.as_ref().map(|p| p.decision),
        Some(Decision::Halt)
    );
    assert_eq!(output.outcome, Some("original"));
    assert!(output.dispatched);
    assert_eq!(output.bypasses, 0);
    let composed = output.compose(audit::draft()?);
    assert_eq!(composed.decision, Verdict::Blocked);
    assert_eq!(
        composed.detail.as_deref().map(String::as_str),
        Some("drift_halted")
    );
    let drift = composed.drift.as_ref().ok_or("drift")?;
    assert_eq!(drift.class, DriftClass::Semantic);
    assert_eq!(drift.side, DriftSide::Both);
    assert!(
        drift
            .changes
            .iter()
            .any(|c| c.kind.as_str() == "malformed_body")
    );
    assert!(
        drift
            .changes
            .iter()
            .any(|c| c.kind.as_str() == "unit_changed")
    );
    let mut publisher = agw_audit::publish::Publisher::new("tenant", Default::default())?;
    publisher.publish(composed.clone(), composed.ts)?;
    assert_eq!(
        publisher
            .store()
            .get(composed.id.as_str(), composed.ts)
            .map(|a| a.decision),
        Some(Verdict::Blocked)
    );
    Ok(())
}

#[test]
fn malformed_write_without_halt_still_reports_bypassed() -> Result {
    let engine = Engine::statistical(Plan::new(identity(), vec![])?);
    let output = engine
        .begin_call(
            context("repair")?,
            Operation::Write,
            "audit",
            1_000_000_000,
            0,
        )
        .execute(body(b"{"), None, || ("original", body(b"{}")));
    let composed = output.compose(audit::draft()?);
    assert_eq!(composed.decision, Verdict::Bypassed);
    assert_eq!(
        composed.detail.as_deref().map(String::as_str),
        Some("malformed_body_bypass")
    );
    Ok(())
}
