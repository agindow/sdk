mod support;
use agw_diff::{DriftClass, Evidence as Authority, Node, Side, Snapshot, compare};
use agw_inspector::{Precision, inference::JsonType as T, inspect};
use agw_repair::{Mode, Status};
use support::{evidence, rename, replacement, run, unchanged};

#[test]
fn generated_audit_patch_contains_exact_fields_and_false_byte_identity()
-> Result<(), Box<dyn std::error::Error>> {
    let bytes = br#"{"source":"a"}"#;
    let (report, candidate) = rename("source", "target", "target");
    let outcome = run(bytes, &report, &[candidate], Mode::Strict);
    let draft: agw_audit::contracts::audit_artifact::AuditArtifact = serde_json::from_str(
        include_str!("../../../../../contracts/examples/audit_artifact.valid.json"),
    )?;
    let before = serde_json::to_value(&draft)?;
    let stamped = outcome.audit(draft);
    let value = serde_json::to_value(&stamped)?;
    assert_eq!(value["patch"]["byte_identical"], false);
    assert_eq!(
        value["patch"]["fields_changed"],
        serde_json::json!(["source"])
    );
    assert_eq!(value["patch"]["selective"], true);
    assert_eq!(value["patch"]["applied"], true);
    assert_eq!(value["mode"], "strict");
    assert_eq!(value["confidence"], 1.0);
    assert_eq!(value["proof"], before["proof"]);
    assert_eq!(value["decision"], before["decision"]);
    let _: agw_audit::contracts::audit_artifact::AuditArtifact = serde_json::from_value(value)?;
    Ok(())
}

#[test]
fn rejected_plan_clears_stale_patch_and_confidence_in_an_owned_audit_draft()
-> Result<(), Box<dyn std::error::Error>> {
    let bytes = br#"{"source":"a"}"#;
    let (report, mut candidate) = rename("source", "target", "target");
    candidate.evidence = evidence(0.1);
    let outcome = run(bytes, &report, &[candidate], Mode::Repair);
    let draft = serde_json::from_str(include_str!(
        "../../../../../contracts/examples/audit_artifact.valid.json"
    ))?;
    let value = serde_json::to_value(outcome.audit(draft))?;
    assert!(value["patch"].is_null());
    assert!(value.get("confidence").is_none());
    Ok(())
}

#[test]
fn escaped_empty_unicode_and_nested_keys_patch_the_right_tokens() {
    for (bytes, path, destination, to_key, expected) in [
        (
            br#"{"a.b":{"source":"a"}}"#.as_slice(),
            "a\\.b.source",
            "a\\.b.target",
            "target",
            br#"{"a.b":{"target":"a"}}"#.as_slice(),
        ),
        (
            br#"{"":{"":"a"}}"#.as_slice(),
            "\\e.\\e",
            "\\e.\\$\\[\\]\\\\",
            "$[]\\",
            br#"{"":{"$[]\\":"a"}}"#.as_slice(),
        ),
        (
            "{\"\u{03b1}\":{\"source\":1}}".as_bytes(),
            "α.source",
            "α.新",
            "新",
            "{\"α\":{\"新\":1}}".as_bytes(),
        ),
    ] {
        let (report, candidate) = rename(path, destination, to_key);
        assert_eq!(
            run(bytes, &report, &[candidate], Mode::Repair)
                .body
                .as_ref(),
            expected
        );
    }
}

#[test]
fn no_op_replacements_preserve_the_original_slice_and_have_no_patch() {
    let bytes = br#"{ "amount":1e400 }"#;
    let (report, candidate) = replacement("amount", b"1e400", DriftClass::Semantic);
    let result = run(bytes, &report, &[candidate], Mode::Repair);
    unchanged(&result, bytes);
    assert_eq!(result.assessments[0].status, Status::Noop);
    assert_eq!(result.unresolved.len(), 1);
}

#[test]
fn high_precision_inspection_matches_by_lexeme_without_numeric_coercion() {
    let bytes = br#"{"source":18446744073709551616,"keep":1e400}"#;
    let (report, candidate) = rename("source", "target", "target");
    let result = agw_repair::repair(
        Side::Request,
        bytes,
        &inspect(bytes, false, Some(Precision::HighPrecision)),
        &report,
        &[candidate],
        Mode::Repair,
    );
    assert_eq!(
        result.body.as_ref(),
        br#"{"target":18446744073709551616,"keep":1e400}"#
    );
}

#[test]
fn uses_a_real_ti11_confirmed_rename_report() {
    let mut field = Node::scalar(T::String);
    field.identity = Some("confirmed-payment-token".into());
    let before = Snapshot {
        evidence: Authority::Authoritative,
        node: Node::object([("source".into(), field.clone())]),
    };
    let after = Snapshot {
        evidence: Authority::Authoritative,
        node: Node::object([("payment_method".into(), field)]),
    };
    let report = compare(Side::Request, &before, &after);
    let (_, candidate) = rename("source", "payment_method", "payment_method");
    let bytes = br#"{"source":"a"}"#;
    let outcome = run(bytes, &report, &[candidate], Mode::Repair);
    assert_eq!(outcome.body.as_ref(), br#"{"payment_method":"a"}"#);
    assert!(outcome.unresolved.is_empty());
}
