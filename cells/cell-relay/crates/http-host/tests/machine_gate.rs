#[path = "support/identity.rs"]
mod identity;
use agw_http_host::machine::{Denial, MachineGate, ProofRequest, prove};
use identity::*;
fn manifest() -> String {
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    serde_json::json!({"issuer":context().issuer,"keys":[{"issuer":context().issuer,"kid":"one","x":URL_SAFE_NO_PAD.encode(key().verifying_key().as_bytes()),"until":NOW+300}],"keyless_only":["a"],"legacy_until":NOW+100}).to_string()
}
#[test]
fn gate_requires_proof_and_keyless_only_rejects_legacy_before_guard() -> Result<(), Denial> {
    let mut gate = MachineGate::new(&manifest(), NOW)?;
    assert_eq!(gate.legacy("a", NOW), Err(Denial::Proof));
    assert_eq!(gate.legacy("b", NOW), Ok(()));
    assert_eq!(gate.legacy("b", NOW + 100), Err(Denial::Proof));
    let token = token(&claims())?;
    let req = ProofRequest {
        method: "POST",
        target: "https://agindow.test/v1/guard",
        token: &token,
    };
    let proof = prove(&req, "one", NOW, &key())?;
    assert_eq!(
        gate.admit(&req, None, None, &context(), NOW),
        Err(Denial::Proof)
    );
    gate.admit(&req, Some(&proof), None, &context(), NOW)?;
    gate.refresh(&manifest(), NOW)?;
    assert_eq!(
        gate.admit(&req, Some(&proof), None, &context(), NOW),
        Err(Denial::Replay)
    );
    Ok(())
}
