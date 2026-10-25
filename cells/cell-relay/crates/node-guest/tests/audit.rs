use agw_node_guest::Guest;
use serde_json::Value;

#[test]
fn disabled_publication_never_enqueues() -> Result<(), String> {
    let guest = Guest::new("null", 0.0)?;
    let call = guest.begin(b"{}", false, "observe", "read", 1.0, true, 10000)?;
    call.finish(b"{}", false, 2.0)?;
    assert_eq!(guest.audit_peek(), "null");
    Ok(())
}
#[test]
fn stable_fifo_ack_and_drop_bounds() -> Result<(), String> {
    let guest = Guest::new("null", 0.0)?;
    guest.enable_audit("tenant", "instance", 1, 524288)?;
    for index in 0..2 {
        let mut call = guest.begin(b"{}", false, "observe", "read", index as f64, true, 10000)?;
        call.audit_context("vendor.call", "2026-10-20T12:00:00Z")?;
        call.finish(b"{}", false, 2.0)?;
    }
    let first: Value = serde_json::from_str(&guest.audit_peek()).map_err(|e| e.to_string())?;
    assert_eq!(first["decision"], "ok");
    assert_eq!(first["tool"], "vendor.call");
    assert_eq!(first["metadata"]["tenant"], "tenant");
    assert_eq!(first["proof"]["write_integrity"], false);
    assert!(!guest.audit_ack("wrong"));
    assert_eq!(
        serde_json::from_str::<Value>(&guest.audit_peek()).map_err(|e| e.to_string())?,
        first
    );
    let stats: Value = serde_json::from_str(&guest.audit_stats()).map_err(|e| e.to_string())?;
    assert_eq!(stats["pending"], 1);
    assert_eq!(stats["dropped"], 1);
    assert!(guest.audit_ack(first["id"].as_str().ok_or("id missing")?));
    assert_eq!(guest.audit_peek(), "null");
    Ok(())
}

#[test]
fn request_halts_and_exhausted_budgets_compose_once() -> Result<(), String> {
    let selection = serde_json::json!({ "tenant": "tenant", "connection": "c", "vendor": "v", "operation": "op", "segment": "s",
        "fields": [{ "side": "request", "path": "/value", "baseline": vec!["{\"value\":1}";128],
            "live": vec!["{\"value\":100}";128], "enumeration": true }] }).to_string();
    for (work, decision) in [(200_000_000, "blocked"), (0, "bypassed")] {
        let guest = Guest::new(&selection, 0.0)?;
        guest.enable_audit("tenant", "instance", 4, 524288)?;
        let mut call = guest.begin(
            b"{\"value\":100}",
            false,
            "repair",
            "write",
            1.0,
            true,
            work,
        )?;
        call.audit_context("op", "2026-10-20T12:00:00Z")?;
        call.finish(b"{}", true, 2.0)?;
        let artifact: Value =
            serde_json::from_str(&guest.audit_peek()).map_err(|e| e.to_string())?;
        assert_eq!(artifact["decision"], decision);
        let stats: Value = serde_json::from_str(&guest.audit_stats()).map_err(|e| e.to_string())?;
        assert_eq!(stats["pending"], 1);
    }
    Ok(())
}
#[test]
fn byte_capacity_drops_without_growth_and_ids_preserve_fifo() -> Result<(), String> {
    let guest = Guest::new("null", 0.0)?;
    guest.enable_audit("tenant", "instance", 2, 524288)?;
    let mut ids = Vec::new();
    for _ in 0..2 {
        let mut call = guest.begin(b"{}", false, "observe", "read", 0.0, true, 10000)?;
        call.audit_context("op", "2026-10-20T12:00:00Z")?;
        call.finish(b"{}", false, 0.0)?;
    }
    for _ in 0..2 {
        let artifact: Value =
            serde_json::from_str(&guest.audit_peek()).map_err(|e| e.to_string())?;
        let id = artifact["id"].as_str().ok_or("missing id")?.to_string();
        assert!(guest.audit_ack(&id));
        ids.push(id);
    }
    assert_eq!(ids, ["aud_instance_1", "aud_instance_2"]);
    let small = Guest::new("null", 0.0)?;
    small.enable_audit("tenant", "small", 256, 1024)?;
    for _ in 0..100 {
        let mut call = small.begin(b"{}", false, "observe", "read", 0.0, true, 10000)?;
        call.audit_context("op", "2026-10-20T12:00:00Z")?;
        call.finish(b"{}", false, 0.0)?;
    }
    let stats: Value = serde_json::from_str(&small.audit_stats()).map_err(|e| e.to_string())?;
    assert!(stats["bytes"].as_u64().is_some_and(|bytes| bytes <= 1024));
    assert_eq!(
        stats["dropped"].as_u64().ok_or("drops missing")?
            + stats["pending"].as_u64().ok_or("pending missing")?,
        100
    );
    Ok(())
}
#[test]
fn audit_tenant_must_match_selected_scope() -> Result<(), String> {
    let selection = serde_json::json!({"tenant":"tenant","connection":"c","vendor":"v","operation":"op","segment":"s",
        "fields":[{"side":"request","path":"/value","baseline":["{\"value\":1}"],"live":["{\"value\":1}"],"enumeration":true}]}).to_string();
    let guest = Guest::new(&selection, 0.0)?;
    assert!(guest.enable_audit("other", "instance", 4, 524288).is_err());
    guest.enable_audit("tenant", "instance", 4, 524288)?;
    Ok(())
}
