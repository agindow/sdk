//! TI-36: complete malformed evidence, policy and fault boundaries.
use agw_audit::contracts::audit_artifact::AuditArtifact;
use agw_diff::{Evidence, Node, Side, Snapshot};
use agw_guard::{
    config::{Config, RuntimeConfig},
    malformed::{Body, Disposition, History},
};
use agw_inspector::{Inspection, Parse, ParseClass, ParseFailure};
use std::error::Error;

type Result = std::result::Result<(), Box<dyn Error>>;

fn draft(operation: &str) -> std::result::Result<AuditArtifact, serde_json::Error> {
    let mut value: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../contracts/examples/audit_artifact.valid.json"
    ))?;
    value["operation"] = operation.into();
    value["hypothesis"] = "stale prior hypothesis".into();
    value["note"] = "stale prior note".into();
    value["detail"] = "stale prior detail".into();
    value["confidence"] = 1.0.into();
    serde_json::from_value(value)
}
fn baseline() -> Snapshot {
    Snapshot {
        node: Node::object([]),
        evidence: Evidence::Authoritative,
    }
}
fn body<'a>(bytes: &'a [u8], base: &'a Snapshot, side: Side) -> Body<'a> {
    Body {
        bytes,
        baseline: base,
        side,
        truncated: false,
        scope: "tenant-a/connection-a",
        vendor: "stripe",
    }
}
fn runtime(mode: &str) -> std::result::Result<RuntimeConfig, agw_guard::config::ConfigError> {
    RuntimeConfig::from_sources(
        Config {
            mode: Some(mode.into()),
            ..Config::default()
        },
        None,
    )
}

#[test]
fn malformed_matrix_returns_original_bytes_and_exact_audit() -> Result {
    let base = baseline();
    for bytes in [b"{\"a\":".as_slice(), b"", b"null", b"[]", b"true", b"\xff"] {
        for side in [Side::Request, Side::Response] {
            for operation in ["read", "write", "unknown"] {
                for mode in ["observe", "repair", "strict"] {
                    let call = runtime(mode)?.begin_call(None)?;
                    let outcome = call.malformed(
                        body(bytes, &base, side),
                        &mut History::default(),
                        draft(operation)?,
                    );
                    assert!(std::ptr::eq(outcome.body, bytes));
                    let expected = if mode == "strict" {
                        Disposition::Halt
                    } else if operation == "read" {
                        Disposition::Degraded
                    } else {
                        Disposition::Bypassed
                    };
                    assert_eq!(outcome.disposition, expected);
                    assert_eq!(outcome.headers.get("X-Agindow-Repair"), Some("skipped"));
                    assert_eq!(outcome.headers.get("X-Agindow-Drift"), Some("detected"));
                    assert_eq!(outcome.headers.get("X-Agindow-Drift-Class"), Some("shape"));
                    assert_eq!(outcome.headers.get("X-Agindow-Mode"), Some(mode));
                    let status = if mode == "strict" {
                        "blocked"
                    } else if operation == "read" {
                        "degraded"
                    } else {
                        "bypassed"
                    };
                    assert_eq!(outcome.headers.get("X-Agindow-Status"), Some(status));
                    assert!(outcome.warning.is_some());
                    let audit = serde_json::to_value(outcome.audit)?;
                    assert_eq!(audit["decision"], status);
                    assert_eq!(audit["mode"], mode);
                    assert_eq!(audit["confidence"], 0.0);
                    assert_eq!(
                        audit["drift"]["changes"],
                        serde_json::json!([{"path":"$","kind":"malformed_body"}])
                    );
                    assert_eq!(audit["drift"]["class"], "shape");
                    assert_eq!(
                        audit["drift"]["side"],
                        if side == Side::Request {
                            "request"
                        } else {
                            "response"
                        }
                    );
                    assert_eq!(audit["patch"]["applied"], false);
                    assert_eq!(audit["patch"]["byte_identical"], true);
                    assert_eq!(audit["proof"]["write_integrity"], false);
                    assert!(audit.get("hypothesis").is_none());
                    assert!(audit.get("note").is_none());
                    assert_eq!(audit["detail"], "malformed_body");
                }
            }
        }
    }
    Ok(())
}

#[test]
fn incomplete_captures_skip_parser_and_never_claim_drift_in_any_mode() -> Result {
    let base = baseline();
    for bytes in [b"{".as_slice(), b"{}"] {
        for mode in ["observe", "repair", "strict"] {
            let call = runtime(mode)?.begin_call(None)?;
            let mut input = body(bytes, &base, Side::Response);
            input.truncated = true;
            let outcome =
                call.malformed_with(input, &mut History::default(), draft("read")?, |_, _| {
                    panic!("incomplete capture must not be parsed")
                });
            assert_eq!(outcome.disposition, Disposition::Incomplete);
            assert!(std::ptr::eq(outcome.body, bytes));
            assert_eq!(outcome.headers.get("X-Agindow-Drift"), None);
            assert_eq!(outcome.headers.get("X-Agindow-Status"), None);
            let audit = serde_json::to_value(outcome.audit)?;
            assert!(audit["drift"].is_null());
            assert!(audit["confidence"].is_null());
            assert_eq!(audit["detail"], "inspection_incomplete");
            assert_eq!(audit["patch"]["byte_identical"], true);
        }
    }
    Ok(())
}

#[test]
fn internal_parser_panics_and_failures_bypass_even_in_strict_mode() -> Result {
    let base = baseline();
    let bytes = b"{";
    for mode in ["observe", "repair", "strict"] {
        let call = runtime(mode)?.begin_call(None)?;
        for class in [None, Some(ParseClass::Data), Some(ParseClass::Io)] {
            let outcome = call.malformed_with(
                body(bytes, &base, Side::Request),
                &mut History::default(),
                draft("write")?,
                |_, _| {
                    let Some(class) = class else {
                        panic!("parser fault")
                    };
                    Inspection {
                        truncated: false,
                        parse: Parse::Unparseable(ParseFailure {
                            class,
                            line: 0,
                            column: 0,
                            message: "internal".into(),
                        }),
                    }
                },
            );
            assert_eq!(outcome.disposition, Disposition::Bypassed);
            assert!(std::ptr::eq(outcome.body, bytes));
            assert_eq!(outcome.headers.get("X-Agindow-Status"), Some("bypassed"));
            assert_eq!(outcome.headers.get("X-Agindow-Drift"), None);
            let audit = serde_json::to_value(outcome.audit)?;
            assert!(audit["drift"].is_null());
            assert_eq!(audit["decision"], "bypassed");
            assert_eq!(audit["detail"], "inspection_fault");
        }
    }
    Ok(())
}

#[test]
fn recurrence_accumulates_only_findings_and_resets_when_scope_changes() -> Result {
    let base = baseline();
    let call = runtime("repair")?.begin_call(None)?;
    let mut history = History::default();
    for occurrence in 1..=3 {
        let mut artifact = draft("write")?;
        artifact.ts += chrono::Duration::seconds(occurrence);
        let first = draft("write")?.ts + chrono::Duration::seconds(1);
        let outcome = call.malformed(body(b"{", &base, Side::Request), &mut history, artifact);
        let audit = serde_json::to_value(outcome.audit)?;
        assert_eq!(audit["drift"]["occurrences"], occurrence);
        assert_eq!(audit["drift"]["first_seen"], serde_json::to_value(first)?);
        let mut incomplete = body(b"{}", &base, Side::Request);
        incomplete.truncated = true;
        let _ = call.malformed(incomplete, &mut history, draft("write")?);
    }
    for scope in ["tenant-b", "tenant-c"] {
        let mut input = body(b"{", &base, Side::Request);
        input.scope = scope;
        let outcome = call.malformed(input, &mut history, draft("write")?);
        assert_eq!(
            serde_json::to_value(outcome.audit)?["drift"]["occurrences"],
            1
        );
    }
    Ok(())
}

#[test]
fn other_drift_and_clean_bodies_are_left_for_the_normal_pipeline() -> Result {
    let base = baseline();
    let call = runtime("strict")?.begin_call(None)?;
    for bytes in [b"{}".as_slice(), b"{\"extra\":1}"] {
        let outcome = call.malformed(
            body(bytes, &base, Side::Request),
            &mut History::default(),
            draft("write")?,
        );
        assert_eq!(outcome.disposition, Disposition::NotApplicable);
        assert!(outcome.audit.is_none());
        assert!(std::ptr::eq(outcome.body, bytes));
        assert_eq!(outcome.headers.iter().count(), 0);
    }
    Ok(())
}

#[test]
fn cap_and_transport_cut_heads_remain_incomplete_end_to_end() -> Result {
    use agw_inspector::tee::Tee;
    use std::io::{self, Cursor, Read};
    struct Failing {
        head: Cursor<Vec<u8>>,
    }
    impl Read for Failing {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if self.head.position() == self.head.get_ref().len() as u64 {
                Err(io::Error::new(
                    io::ErrorKind::ConnectionReset,
                    "vendor reset",
                ))
            } else {
                self.head.read(buf)
            }
        }
    }
    let base = baseline();
    let call = runtime("strict")?.begin_call(None)?;
    for head in [b"{".as_slice(), b"{}"] {
        let mut full = head.to_vec();
        full.extend_from_slice(b" trailing vendor bytes");
        let mut capped = Tee::with_cap(Cursor::new(full.clone()), head.len());
        let mut forwarded = Vec::new();
        capped.read_to_end(&mut forwarded)?;
        assert_eq!(forwarded, full);
        let mut failed = Tee::new(Failing {
            head: Cursor::new(head.to_vec()),
        });
        let mut received = Vec::new();
        let error = failed
            .read_to_end(&mut received)
            .err()
            .ok_or("expected transport error")?;
        assert_eq!(error.kind(), io::ErrorKind::ConnectionReset);
        assert_eq!(received, head);
        for inspection in [capped.into_inspection(), failed.into_inspection()] {
            assert!(inspection.truncated);
            let outcome = call.malformed_with(
                body(head, &base, Side::Response),
                &mut History::default(),
                draft("read")?,
                |_, _| inspection,
            );
            assert_eq!(outcome.disposition, Disposition::Incomplete);
            let audit = serde_json::to_value(outcome.audit)?;
            assert!(audit["drift"].is_null());
            assert_eq!(audit["detail"], "inspection_incomplete");
        }
    }
    Ok(())
}

#[test]
fn history_key_isolates_vendor_tool_operation_side_and_baseline() -> Result {
    let base = baseline();
    let call = runtime("repair")?.begin_call(None)?;
    let mut history = History::default();
    for dimension in ["initial", "vendor", "tool", "operation", "side", "baseline"] {
        let mut input = body(b"{", &base, Side::Request);
        let mut artifact = draft("write")?;
        // Cumulative key changes ensure each dimension alone resets the slot.
        if dimension != "initial" {
            input.vendor = "other";
        }
        if ["tool", "operation", "side", "baseline"].contains(&dimension) {
            artifact.tool = "other.tool".try_into()?;
        }
        if ["operation", "side", "baseline"].contains(&dimension) {
            artifact.operation = "read".parse()?;
        }
        if ["side", "baseline"].contains(&dimension) {
            input.side = Side::Response;
        }
        if dimension == "baseline" {
            artifact.proof.contract_version = "v2".try_into()?;
        }
        for count in 1..=2 {
            let outcome = call.malformed(input, &mut history, artifact.clone());
            assert_eq!(
                serde_json::to_value(outcome.audit)?["drift"]["occurrences"],
                count,
                "{dimension}"
            );
        }
    }
    Ok(())
}
