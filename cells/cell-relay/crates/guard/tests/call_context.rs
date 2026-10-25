//! The mode in the proof must be the mode that produced the call.
mod support;

use agw_audit::contracts::audit_artifact::{AuditArtifact, AuditArtifactDecision};
use agw_guard::config::{Config, RuntimeConfig, X_AGINDOW_MODE};
use agw_guard::proxy::{Headers, UpstreamResponse, X_AGINDOW_LATENCY_MS};
use std::cell::Cell;
use std::error::Error;
use std::io::{Cursor, Read};
use std::rc::Rc;
use std::time::Duration;
use support::SpyReader;

type Result = std::result::Result<(), Box<dyn Error>>;
const ARTIFACT: &str = include_str!("../../../../../contracts/examples/audit_artifact.valid.json");

#[test]
fn every_audit_decision_serializes_the_call_snapshot() -> Result {
    for mode in ["observe", "repair", "strict"] {
        for overridden in [false, true] {
            let runtime = RuntimeConfig::from_sources(
                Config {
                    mode: Some(mode.to_owned()),
                    ..Config::default()
                },
                None,
            )?;
            let call = if overridden {
                runtime.reload_mode("repair")?;
                runtime.begin_call(Some(mode))?
            } else {
                runtime.begin_call(None)?
            };
            runtime.reload_mode("strict")?;
            for decision in [
                AuditArtifactDecision::Ok,
                AuditArtifactDecision::Repaired,
                AuditArtifactDecision::Degraded,
                AuditArtifactDecision::Bypassed,
                AuditArtifactDecision::Blocked,
            ] {
                let mut draft: AuditArtifact = serde_json::from_str(ARTIFACT)?;
                draft.decision = decision;
                let mut expected = serde_json::to_value(&draft)?;
                expected["mode"] = serde_json::json!(mode);
                let artifact = call.audit(draft);
                assert_eq!(serde_json::to_value(&artifact)?, expected);
            }
        }
    }
    Ok(())
}

#[test]
fn forwarding_echoes_snapshot_claims_vendor_headers_and_never_reads_ahead() -> Result {
    let payload = b" {\"id\":18446744073709551615,\"x\":1.2300} \n";
    for mode in ["observe", "repair", "strict"] {
        let runtime = RuntimeConfig::from_sources(Config::default(), None)?;
        let call = runtime.begin_call(Some(mode))?;
        runtime.reload_mode("strict")?;
        let reads = Rc::new(Cell::new(0));
        let upstream = UpstreamResponse {
            status: 503,
            headers: Headers::from(vec![
                ("Set-Cookie".into(), "a=1".into()),
                ("x-agindow-mode".into(), "spoofed".into()),
                ("Set-Cookie".into(), "b=2".into()),
                ("X-AGINDOW-MODE".into(), "observe".into()),
                ("x-agindow-latency-ms".into(), "999".into()),
            ]),
            body: SpyReader {
                inner: Cursor::new(payload),
                reads: Rc::clone(&reads),
                max_requested: Rc::new(Cell::new(0)),
            },
        };
        let mut response = call.forward(upstream, Duration::from_millis(3));
        assert_eq!(reads.get(), 0);
        assert_eq!(response.status, 503);
        assert_eq!(
            response.headers.iter().collect::<Vec<_>>(),
            vec![
                ("Set-Cookie", "a=1"),
                ("Set-Cookie", "b=2"),
                (X_AGINDOW_LATENCY_MS, "3"),
                (X_AGINDOW_MODE, mode),
            ]
        );
        let mut output = Vec::new();
        response.body.read_to_end(&mut output)?;
        assert_eq!(output, payload);
    }
    Ok(())
}

#[test]
fn configured_capture_cap_is_wired_to_the_tee_without_cutting_the_stream() -> Result {
    for cap in [0, 1, 8, 32] {
        let runtime = RuntimeConfig::from_sources(
            Config {
                inspection_cap: Some(cap),
                ..Config::default()
            },
            None,
        )?;
        let call = runtime.begin_call(None)?;
        let payload = vec![0xA5; 64];
        let upstream = UpstreamResponse {
            status: 200,
            headers: Headers::new(),
            body: Cursor::new(payload.clone()),
        };
        let mut response = call.forward(upstream, Duration::ZERO);
        let mut output = Vec::new();
        response.body.read_to_end(&mut output)?;
        assert_eq!(output, payload);
        let tee = response.body.into_inner();
        assert_eq!(tee.cap(), cap);
        assert_eq!(tee.captured(), &payload[..cap]);
        assert!(tee.truncated());
    }
    Ok(())
}
