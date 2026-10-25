#[path = "support/identity.rs"]
mod identity;
use agw_guard::statistical::Engine;
use agw_http_host::{Envelope, execution::Execution};
#[test]
fn authority_audit_contains_actor_ids_and_scope_but_never_token_values()
-> Result<(), Box<dyn std::error::Error>> {
    let input: Envelope = serde_json::from_str(
        r#"{"tool":"read","connection":"c","operation":"read","args":{},"mode":"observe"}"#,
    )?;
    let engine = Engine::NullEngine;
    let authority = identity::claims();
    let call = Execution::start(&input, &engine, "audit".into(), 1000)?
        .with_machine_authority(&authority)?;
    let done = call.finish(b"{}", false)?;
    let bytes = serde_json::to_string(&done.artifact)?;
    assert!(bytes.contains("access-1"));
    assert!(bytes.contains("machine_auth"));
    assert!(!bytes.contains(&identity::token(&authority)?));
    Ok(())
}
