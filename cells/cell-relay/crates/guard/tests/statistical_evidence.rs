#[path = "support/statistical.rs"]
mod support;
use agw_diff::Side;
use agw_guard::statistical::{Decision, Detector, Engine, Materiality, Operation, Plan};
use agw_stats::control::{Decision as Eligibility, Signal};
use support::{body, context, field, identity};
type Result = std::result::Result<(), Box<dyn std::error::Error>>;
#[test]
fn insufficient_and_suppressed_evidence_never_becomes_drift() -> Result {
    for kind in ["clean", "insufficient", "insignificant", "below_floor"] {
        let selected = match kind {
            "clean" => field(Side::Request, "1", "1")?,
            "insufficient" => support::field_samples(Side::Request, "1", "100", 10)?,
            "below_floor" => field(Side::Request, "1", "null")?,
            _ => field(Side::Request, "1", "100")?,
        };
        let mut plan = Plan::new(identity(), vec![selected])?;
        if kind == "insignificant" {
            plan.alpha = 1e-25;
        }
        if kind == "below_floor" {
            plan.overrides.push(Materiality {
                field: "/value".into(),
                signal: Signal::Completeness,
                floor: 1.0,
            });
        }
        let engine = Engine::statistical(plan);
        let output = engine
            .begin_call(
                context("strict")?,
                Operation::Write,
                "audit",
                100_000_000,
                0,
            )
            .execute(body(b"{\"value\":1}"), None, || ("original", body(b"{}")));
        assert_eq!(output.bypasses, 0, "{kind}");
        assert_eq!(output.request.decision, Decision::Pass, "{kind}");
        assert!(output.request.report.changes.is_empty(), "{kind}");
        assert_eq!(output.outcome, Some("original"));
        if kind == "below_floor" {
            assert!(
                output
                    .request
                    .records
                    .iter()
                    .flat_map(|r| &r.eligibility)
                    .any(|d| *d == Some(Eligibility::BelowMateriality))
            );
        }
        if kind == "insignificant" {
            assert!(
                output
                    .request
                    .records
                    .iter()
                    .flat_map(|r| &r.eligibility)
                    .any(|d| *d == Some(Eligibility::NotSignificant))
            );
        }
        if kind == "insufficient" {
            assert!(
                output
                    .request
                    .records
                    .iter()
                    .all(|r| !r.evidence.sufficient())
            );
        }
    }
    Ok(())
}
#[test]
fn family_is_frozen_across_both_sides_before_filtering_and_enum_requires_opt_in() -> Result {
    let mut request = field(Side::Request, "\"old\"", "\"new\"")?;
    request.enumeration = false;
    let response = field(Side::Response, "1", "1")?;
    let engine = Engine::statistical(Plan::new(identity(), vec![request, response])?);
    let output = engine
        .begin_call(
            context("observe")?,
            Operation::Read,
            "audit",
            100_000_000,
            0,
        )
        .execute(body(b"{\"value\":\"new\"}"), None, || {
            ("original", body(b"{\"value\":1}"))
        });
    assert_eq!(output.family, 1070);
    assert!(
        !output
            .request
            .records
            .iter()
            .any(|r| r.detector == Detector::Enumeration)
    );
    for record in output
        .request
        .records
        .iter()
        .chain(&output.response.ok_or("response")?.records)
    {
        for test in record.evidence.tests() {
            assert_eq!(test.evidence().tests, 1070);
        }
    }
    Ok(())
}
#[test]
fn unsafe_numeric_projection_skips_only_unit_and_preserves_exact_payload() -> Result {
    let engine = Engine::statistical(Plan::new(
        identity(),
        vec![field(Side::Request, "1", "100")?],
    )?);
    let bytes = b"{\"value\":9007199254740993}";
    let output = engine
        .begin_call(
            context("observe")?,
            Operation::Write,
            "audit",
            100_000_000,
            0,
        )
        .execute(body(bytes), None, || (bytes.as_slice(), body(b"{}")));
    assert_eq!(output.bypasses, 0);
    assert_eq!(output.request.records.len(), 4);
    assert!(
        !output
            .request
            .records
            .iter()
            .any(|r| r.detector == Detector::Unit)
    );
    assert!(std::ptr::eq(
        output.outcome.ok_or("outcome")?,
        bytes.as_slice()
    ));
    Ok(())
}
#[test]
fn one_work_budget_covers_both_phases_and_response_exhaustion_preserves_attempt() -> Result {
    let engine = Engine::statistical(Plan::new(
        identity(),
        vec![
            field(Side::Request, "1", "1")?,
            field(Side::Response, "1", "100")?,
        ],
    )?);
    let output = engine
        .begin_call(context("strict")?, Operation::Write, "audit", 30_000_000, 0)
        .execute(body(b"{\"value\":1}"), None, || {
            ("completed", body(b"{\"value\":100}"))
        });
    assert_eq!(output.request.records.len(), 5);
    let response = output.response.ok_or("response")?;
    assert_eq!(
        response.fault,
        Some(agw_guard::availability::EngineFault::ResourceExhausted)
    );
    assert!(response.records.is_empty());
    assert_eq!(output.bypasses, 1);
    assert_eq!(output.outcome, Some("completed"));
    Ok(())
}
#[test]
fn plan_rejects_cross_tenant_snapshots_duplicate_fields_and_oversized_families() -> Result {
    let selected = field(Side::Request, "1", "1")?;
    let mut other = identity();
    other.tenant = "other".into();
    assert!(Plan::new(other, vec![selected.clone()]).is_err());
    assert!(Plan::new(identity(), vec![selected.clone(), selected.clone()]).is_err());
    assert!(Plan::new(identity(), vec![selected; 8]).is_err());
    Ok(())
}

#[test]
fn current_call_extraction_completes_the_actual_detector_window_without_collecting_a_baseline()
-> Result {
    let selected = support::field_counts(Side::Request, "1", "100", 128, 19)?;
    let agw_stats::windows::Availability::Ready(snapshot) = &selected.snapshot else {
        return Err("snapshot".into());
    };
    let retained = snapshot.clone();
    let engine = Engine::statistical(Plan::new(identity(), vec![selected])?);
    let output = engine
        .begin_call(
            context("repair")?,
            Operation::Write,
            "audit",
            100_000_000,
            0,
        )
        .execute::<(), _>(body(b"{\"value\":100}"), None, || {
            panic!("current sample makes the material finding sufficient")
        });
    assert_eq!(output.request.decision, Decision::Halt);
    let record = output
        .request
        .records
        .iter()
        .find(|r| r.detector == Detector::Unit)
        .ok_or("unit")?;
    assert_eq!(record.evidence.tests()[0].samples(), (128, 20));
    assert_eq!(retained.baseline().samples().len(), 128);
    assert_eq!(retained.live().samples().len(), 19);
    Ok(())
}
