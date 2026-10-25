//! TI-47: one composed statistical audit artifact per call (contract §10.41).
//!
//! The tests pin the composition boundary, not a new detector: all admitted
//! evidence survives and stale draft claims do not.
#[path = "support/audit.rs"]
mod audit;
#[path = "support/statistical.rs"]
mod support;
use agw_audit::contracts::audit_artifact::{
    AuditArtifactDecision as Verdict, AuditArtifactMode as Mode, Patch, Proof, ProofVerifiedAgainst,
};
use agw_diff::Side;
use agw_guard::statistical::{Applied, Engine, Operation, Plan};
use audit::{draft, finding, findings, statistics};
use support::{body, context, field, field_counts, field_path, identity};
type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

fn permuted(
    side: Side,
    values: &[(&str, &str, &str)],
) -> Result<Vec<agw_guard::statistical::Field>> {
    values
        .iter()
        .map(|(path, baseline, live)| field_path(side, baseline, live, 128, path))
        .collect()
}

#[test]
fn composes_every_admitted_finding_from_both_phases_without_losing_evidence() -> Result {
    let request = [
        ("/amount", "1", "100"),
        ("/status", "\"old\"", "\"\""),
        ("/flag", "1", "null"),
        ("/kind", "1", "\"x\""),
        ("/name", "\"abc\"", "\"2024-01-01\""),
    ];
    let response = [("/amount", "1", "100"), ("/status", "\"old\"", "\"\"")];
    let mut fields = permuted(Side::Request, &request)?;
    fields.extend(permuted(Side::Response, &response)?);
    let engine = Engine::statistical(Plan::new(identity(), fields)?);
    let observation = engine.begin_call(
        context("observe")?,
        Operation::Read,
        "audit",
        1_000_000_000,
        0,
    );
    let output = observation.execute(
        body(br#"{"amount":100,"status":"","flag":null,"kind":"x","name":"2024-01-01"}"#),
        None,
        || ("original", body(br#"{"amount":100,"status":""}"#)),
    );
    let audit = output.compose(draft()?);
    let drift = audit.drift.as_ref().ok_or("drift")?;
    assert_eq!(
        drift.class,
        agw_audit::contracts::audit_artifact::DriftClass::Semantic
    );
    assert_eq!(
        drift.side,
        agw_audit::contracts::audit_artifact::DriftSide::Both
    );
    assert_eq!(audit.decision, Verdict::Ok);
    assert_eq!(audit.mode, Mode::Observe);
    assert!(
        output.dispatched,
        "a response-side finding reports the completed attempt"
    );
    assert!(audit.patch.is_none(), "diagnosis-only call applies nothing");

    let unit = finding(&audit, "unit", "/amount")?;
    let hypothesis = &unit["hypotheses"][0];
    assert_eq!(hypothesis["kind"], "unit_conversion");
    assert_eq!(hypothesis["baseline_median"], 1.0);
    assert_eq!(hypothesis["observed_median"], 100.0);
    assert_eq!(hypothesis["ratio"], 100.0);
    assert_eq!(hypothesis["tolerance"], 0.02);
    let enumeration = finding(&audit, "enumeration", "/status")?;
    assert_eq!(enumeration["reason"]["kind"], "new_value");
    assert_eq!(enumeration["reason"]["value"], "");
    assert_eq!(
        audit
            .hypothesis
            .as_ref()
            .map(|hypothesis| hypothesis.to_string())
            .as_deref(),
        Some("unit_conversion"),
        "one hypothesis kind is promoted to the artifact-level slot"
    );
    assert!(enumeration["reason"]["window_first_seen"].is_string());
    assert!(
        enumeration["reason"].get("first_seen").is_none(),
        "a window timestamp is never presented as persistent first-ever history"
    );
    let completeness = finding(&audit, "completeness", "/flag")?;
    assert_eq!(completeness["reason"]["kind"], "completeness_delta");
    assert_eq!(completeness["reason"]["rates"]["baseline"], 1.0);
    assert_eq!(completeness["reason"]["rates"]["observed"], 0.0);
    let kind = finding(&audit, "type_consistency", "/kind")?;
    assert_eq!(kind["reason"]["kind"], "type_consistency");
    let charset = finding(&audit, "charset", "/name")?;
    assert_eq!(charset["reason"]["kind"], "charset_shift");
    let format = findings(&audit)?
        .iter()
        .find(|entry| entry["path"] == "/name" && entry["reason"]["kind"] == "format_emerged")
        .ok_or("missing anchored format finding")?;
    let pattern = format["reason"]["pattern"]
        .as_str()
        .ok_or("missing format pattern")?;
    assert!(pattern.starts_with('^') && pattern.ends_with('$'));
    assert_eq!(
        statistics(&audit)?["note"],
        "statistical evidence only; no conversion, repair or write-integrity permission"
    );

    let second = Engine::statistical(Plan::new(identity(), {
        let mut fields = permuted(Side::Request, &request)?;
        fields.extend(permuted(Side::Response, &response)?);
        fields.reverse();
        fields
    })?);
    let output = second.begin_call(
        context("observe")?,
        Operation::Read,
        "audit",
        1_000_000_000,
        0,
    );
    let output = output.execute(
        body(br#"{"amount":100,"status":"","flag":null,"kind":"x","name":"2024-01-01"}"#),
        None,
        || ("original", body(br#"{"amount":100,"status":""}"#)),
    );
    let reordered = output.compose(draft()?);
    assert_eq!(
        findings(&audit)?,
        findings(&reordered)?,
        "order cannot change evidence"
    );
    assert_eq!(
        serde_json::to_value(&audit.drift.as_ref().ok_or("drift")?.changes)?,
        serde_json::to_value(&reordered.drift.as_ref().ok_or("drift")?.changes)?,
        "order cannot change change reporting"
    );
    Ok(())
}

#[test]
fn blocked_unit_diagnosis_reports_no_patch_and_no_fabricated_integrity() -> Result {
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
            0,
        )
        .execute(body(b"{\"value\":100}"), None, || ("original", body(b"{}")));
    assert!(!output.dispatched);
    let audit = output.compose(draft()?);
    assert_eq!(audit.decision, Verdict::Blocked);
    assert_eq!(
        audit.operation,
        agw_audit::contracts::audit_artifact::AuditArtifactOperation::Write
    );
    assert_eq!(
        audit.hypothesis.as_deref().map(String::as_str),
        Some("unit_conversion")
    );
    assert!(audit.patch.is_none());
    assert!(!audit.proof.write_integrity);
    assert!(audit.proof.idempotency_key.is_none());
    assert_eq!(audit.confidence, None);
    let unit = finding(&audit, "unit", "/value")?;
    assert_eq!(unit["reason"]["kind"], "magnitude_shift");
    Ok(())
}

#[test]
fn reused_draft_evidence_is_replaced_by_the_actual_call_disposition() -> Result {
    let engine = Engine::statistical(Plan::new(
        identity(),
        vec![field(Side::Request, "1", "1")?],
    )?);
    let output = engine
        .begin_call(context("repair")?, Operation::Read, "audit", 100_000_000, 0)
        .execute(body(b"{\"value\":1}"), None, || {
            ("original", body(b"{\"value\":1}"))
        });
    let mut stale = draft()?;
    stale.hypothesis = Some("unit_conversion".try_into()?);
    stale.detail = Some("stale_detail".try_into()?);
    stale.note = Some("stale_note".try_into()?);
    stale.confidence = Some(0.99);
    stale.patch = Some(Patch {
        applied: true,
        byte_identical: false,
        fields_changed: vec!["/value".try_into()?],
        selective: Some(true),
        reasoning: None,
    });
    stale.proof = Proof {
        contract_version: "v1".try_into()?,
        verified_against: ProofVerifiedAgainst::Registry,
        write_integrity: true,
        idempotency_key: Some("stale-key".try_into()?),
    };
    stale.metadata.insert(
        "agindow_statistical".into(),
        serde_json::json!({"findings": [{"stale": true}]}),
    );
    let audit = output.compose(stale);
    assert_eq!(audit.decision, Verdict::Ok);
    assert!(audit.drift.is_none(), "clean call states no drift");
    assert!(audit.patch.is_none(), "stale applied patch is cleared");
    assert!(!audit.proof.write_integrity);
    assert!(audit.proof.idempotency_key.is_none());
    assert_eq!(audit.confidence, None);
    assert_eq!(audit.hypothesis, None);
    assert_eq!(audit.note, None);
    assert_ne!(
        audit.detail.as_deref().map(String::as_str),
        Some("stale_detail")
    );
    assert!(
        findings(&audit)?.is_empty(),
        "stale detector evidence never survives"
    );
    assert_eq!(
        statistics(&audit)?["phases"]["request"]["summary"]["admitted"],
        0
    );

    let bare = Engine::default()
        .begin_call(context("strict")?, Operation::Write, "audit", 0, 0)
        .execute::<&str, _>(body(b"{}"), None, || ("untouched", body(b"{}")));
    let audit = bare.compose(draft()?);
    assert_eq!(audit.decision, Verdict::Ok);
    assert!(audit.drift.is_none());
    assert!(findings(&audit)?.is_empty());
    Ok(())
}

#[test]
fn insufficient_evidence_is_provenance_not_a_clean_finding() -> Result {
    let engine = Engine::statistical(Plan::new(
        identity(),
        vec![field_counts(Side::Request, "1", "100", 128, 8)?],
    )?);
    let output = engine
        .begin_call(
            context("observe")?,
            Operation::Read,
            "audit",
            1_000_000_000,
            0,
        )
        .execute(body(b"{\"value\":100}"), None, || ("original", body(b"{}")));
    let audit = output.compose(draft()?);
    let request = &statistics(&audit)?["phases"]["request"];
    assert_eq!(request["evaluated"].as_bool(), Some(true));
    let insufficient = request["summary"]["insufficient_records"]
        .as_u64()
        .ok_or("insufficient count")?;
    assert!(
        insufficient > 0,
        "unavailable evidence is counted, not dropped"
    );
    assert_eq!(request["summary"]["admitted"], 0);
    assert!(
        findings(&audit)?.is_empty(),
        "insufficient evidence is never reported as a finding"
    );
    assert!(
        audit.drift.is_none(),
        "unavailable evidence is not clean drift"
    );
    Ok(())
}

#[test]
fn availability_bypass_publishes_bypassed_with_no_stale_evidence() -> Result {
    let engine = Engine::statistical(Plan::new(
        identity(),
        vec![field(Side::Request, "1", "100")?],
    )?);
    engine.set_ready(false);
    let mut calls = 0;
    let output = engine
        .begin_call(
            context("strict")?,
            Operation::Write,
            "audit",
            100_000_000,
            0,
        )
        .execute(body(b"{\"value\":100}"), None, || {
            calls += 1;
            ("original", body(b"{}"))
        });
    assert_eq!(calls, 1);
    assert_eq!(output.bypasses, 1);
    let mut stale = draft()?;
    stale.drift = Some(agw_audit::contracts::audit_artifact::Drift {
        class: agw_audit::contracts::audit_artifact::DriftClass::Semantic,
        side: agw_audit::contracts::audit_artifact::DriftSide::Request,
        vendor: None,
        changes: vec![agw_audit::contracts::audit_artifact::Change {
            path: "/value".try_into()?,
            kind: "unit_changed".try_into()?,
            from: None,
            to: None,
        }],
        first_seen: None,
        occurrences: None,
    });
    let audit = output.compose(stale);
    assert_eq!(audit.decision, Verdict::Bypassed);
    assert_eq!(
        audit.detail.as_deref().map(String::as_str),
        Some("availability_bypass")
    );
    assert!(
        audit.drift.is_none(),
        "bypass retains no stale detector drift"
    );
    assert!(audit.patch.is_none());
    assert_eq!(statistics(&audit)?["policy"]["bypasses"], 1);
    assert!(statistics(&audit)?["phases"]["request"]["fault"].is_string());
    Ok(())
}

#[test]
fn verified_applied_repairs_are_reported_and_unverified_ones_are_not() -> Result {
    let engine = Engine::statistical(Plan::new(
        identity(),
        vec![field(Side::Request, "1", "1")?],
    )?);
    let output = engine
        .begin_call(
            context("observe")?,
            Operation::Read,
            "audit",
            100_000_000,
            0,
        )
        .execute(body(b"{\"value\":1}"), None, || {
            ("original", body(b"{\"value\":1}"))
        });
    let unverified = output.compose_with(
        draft()?,
        &[Applied {
            side: Side::Request,
            fields_changed: &["/value".into()],
            verified: false,
            selective: true,
        }],
    );
    assert!(
        unverified.patch.is_none(),
        "unverified application is never reported"
    );
    let applied = output.compose_with(
        draft()?,
        &[Applied {
            side: Side::Request,
            fields_changed: &["/value".into()],
            verified: true,
            selective: true,
        }],
    );
    let patch = applied.patch.as_ref().ok_or("patch")?;
    assert!(patch.applied);
    assert!(!patch.byte_identical);
    assert_eq!(patch.selective, Some(true));
    assert!(
        !applied.proof.write_integrity,
        "application is not write-integrity proof"
    );
    let mixed = output.compose_with(
        draft()?,
        &[
            Applied {
                side: Side::Request,
                fields_changed: &["/value".into()],
                verified: true,
                selective: true,
            },
            Applied {
                side: Side::Response,
                fields_changed: &["/other".into()],
                verified: true,
                selective: false,
            },
        ],
    );
    assert_eq!(
        mixed.patch.as_ref().ok_or("mixed patch")?.selective,
        Some(false),
        "one non-selective applied patch makes the combined claim non-selective"
    );
    Ok(())
}
