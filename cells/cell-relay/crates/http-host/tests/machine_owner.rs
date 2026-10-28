#[path = "support/identity.rs"]
mod identity;
use agw_http_host::machine::{Denial, MachineGate, OwnerEpoch, ProofRequest, prove, sign};
use ed25519_dalek::SigningKey;
use identity::*;
#[test]
fn losing_owner_state_invalidates_old_tokens_instead_of_accepting_old_proofs() -> Result<(), Denial>
{
    let first = OwnerEpoch::new(&"a".repeat(64), [17; 32])?;
    let replacement = OwnerEpoch::new(&"a".repeat(64), [18; 32])?;
    let issuer = context().issuer;
    let token = sign(&claims(), first.kid(), &SigningKey::from_bytes(&[17; 32]))?;
    let request = ProofRequest {
        method: "POST",
        target: "https://agindow.test/v1/guard",
        token: &token,
    };
    let proof = prove(&request, "same-proof", NOW, &key())?;
    let mut original = MachineGate::new(&first.manifest(issuer, &["a".into()], NOW)?, NOW)?;
    original.admit(&request, Some(&proof), None, &context(), NOW)?;
    assert_eq!(
        original.admit(&request, Some(&proof), None, &context(), NOW),
        Err(Denial::Replay)
    );
    let mut fresh = MachineGate::new(&replacement.manifest(issuer, &["a".into()], NOW)?, NOW)?;
    assert_eq!(
        fresh.admit(&request, Some(&proof), None, &context(), NOW),
        Err(Denial::Unavailable)
    );
    assert_ne!(first.kid(), replacement.kid());
    Ok(())
}
