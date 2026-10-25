use agw_http_host::{Envelope, execution::Execution};
use agw_node_guest::local_engine;
use serde_json::{Value, json};
fn selected(side: &str) -> String {
    json!({ "tenant": "tenant", "connection": "conn", "vendor": "v", "operation": "op", "segment": "s",
        "fields": [{ "side": side, "path": "/value", "baseline": vec!["{\"value\":1}";128],
            "live": vec!["{\"value\":100}";128], "enumeration": true }] }).to_string()
}
fn envelope(mode: &str) -> Result<Envelope, serde_json::Error> {
    serde_json::from_value(
        json!({"tool":"op","connection":"conn","operation":"write","args":{"value":100},"mode":mode,"idempotency_key":"same"}),
    )
}
#[test]
fn request_halts_have_exact_feedback_without_dispatch() -> Result<(), Box<dyn std::error::Error>> {
    for (mode, code) in [("strict", "drift_halted"), ("repair", "drift_blocked")] {
        let engine = local_engine(&selected("request"))?;
        let call = Execution::start(&envelope(mode)?, &engine, "aud_test".into(), 200_000_000)?;
        assert!(call.request_halted());
        let done = call.finish(&[], true)?;
        assert!(!done.dispatched);
        assert!(!done.bypassed);
        let feedback = serde_json::to_value(done.feedback.ok_or("feedback missing")?)?;
        assert_eq!(feedback.as_object().map(|v| v.len()), Some(6));
        assert_eq!(feedback["error"], code);
        assert_eq!(feedback["tool"], "op");
        assert_eq!(feedback["drift_class"], "semantic");
    }
    Ok(())
}
#[test]
fn response_halts_retain_dispatch_warning_and_faults_remain_bypasses()
-> Result<(), Box<dyn std::error::Error>> {
    let engine = local_engine(&selected("response"))?;
    let done = Execution::start(
        &envelope("repair")?,
        &engine,
        "aud_test".into(),
        200_000_000,
    )?
    .finish(b"{\"value\":100}", false)?;
    assert!(done.halted && done.dispatched);
    assert!(
        done.feedback
            .ok_or("feedback missing")?
            .hint
            .contains("do not blindly replay")
    );
    let done = Execution::start(&envelope("repair")?, &engine, "aud_test2".into(), 0)?
        .finish(b"{\"value\":100}", false)?;
    assert!(done.bypassed);
    assert!(done.feedback.is_none());
    let artifact: Value = serde_json::to_value(done.artifact)?;
    assert_eq!(artifact["decision"], "bypassed");
    Ok(())
}
