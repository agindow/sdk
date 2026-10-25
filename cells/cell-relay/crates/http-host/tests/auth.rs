use agw_http_host::auth::{Auth, Refusal};
use serde_json::json;
use sha2::{Digest, Sha256};
#[test]
fn scopes_keys_to_routes_and_counts_remaining_with_bounded_clock_recovery()
-> Result<(), &'static str> {
    let digest = format!("{:x}", Sha256::digest(b"fixture"));
    let manifest = json!([{ "digest":digest, "tenant":"t", "routes":["c/op"], "plan":"test", "limit":2,"window_seconds":60 }]).to_string();
    let mut auth = Auth::new(&manifest)?;
    assert!(matches!(
        auth.admit("Bearer bad", "c/op", 120),
        Err(Refusal::Unauthorized)
    ));
    assert!(matches!(
        auth.admit("Bearer fixture", "other/op", 120),
        Err(Refusal::Scope)
    ));
    assert!(
        auth.admit("Bearer fixture", "c/op", 120)
            .is_ok_and(|p| p.remaining == 1 && p.tenant == "t")
    );
    assert!(
        auth.admit("Bearer fixture", "c/op", 120)
            .is_ok_and(|p| p.remaining == 0)
    );
    assert!(matches!(
        auth.admit("Bearer fixture", "c/op", 60),
        Err(Refusal::Limited)
    ));
    assert!(
        auth.admit("Bearer fixture", "c/op", 180)
            .is_ok_and(|p| p.remaining == 1)
    );
    Ok(())
}
