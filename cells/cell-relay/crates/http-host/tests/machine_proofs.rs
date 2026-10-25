#[path = "support/identity.rs"]
mod identity;
use agw_http_host::machine::{
    Confirmation, Denial, ProofRequest, Replay, prove, verify_possession,
};
use ed25519_dalek::SigningKey;
use identity::*;
#[test]
fn stolen_token_cannot_be_used_by_another_key_or_replayed() -> Result<(), Denial> {
    let c = claims();
    let token = token(&c)?;
    let request = ProofRequest {
        method: "POST",
        target: "https://agindow.test/v1/guard",
        token: &token,
    };
    let proof = prove(&request, "proof-1", NOW, &key())?;
    let attacker = prove(&request, "evil-1", NOW, &SigningKey::from_bytes(&[9; 32]))?;
    let mut replay = Replay::new(4)?;
    assert_eq!(
        verify_possession(&c, &request, Some(&attacker), None, &mut replay, NOW),
        Err(Denial::Proof)
    );
    assert_eq!(
        verify_possession(&c, &request, None, None, &mut replay, NOW),
        Err(Denial::Proof)
    );
    verify_possession(&c, &request, Some(&proof), None, &mut replay, NOW)?;
    assert_eq!(
        verify_possession(&c, &request, Some(&proof), None, &mut replay, NOW),
        Err(Denial::Replay)
    );
    let wrong = ProofRequest {
        token: "different",
        ..request
    };
    assert_eq!(
        verify_possession(&c, &wrong, Some(&proof), None, &mut replay, NOW),
        Err(Denial::Proof)
    );
    assert!(cache()?.validate(&token, &context(), NOW).is_ok());
    Ok(())
}
#[test]
fn proof_binds_method_target_time_and_tls_peer() -> Result<(), Denial> {
    let mut c = claims();
    let token = token(&c)?;
    let request = ProofRequest {
        method: "POST",
        target: "https://agindow.test/v1/guard",
        token: &token,
    };
    let proof = prove(&request, "nonce", NOW, &key())?;
    let mut replay = Replay::new(2)?;
    for wrong in [
        ProofRequest {
            method: "GET",
            ..request
        },
        ProofRequest {
            target: "https://agindow.test/else",
            ..request
        },
    ] {
        assert_eq!(
            verify_possession(&c, &wrong, Some(&proof), None, &mut replay, NOW),
            Err(Denial::Proof)
        );
    }
    assert_eq!(
        verify_possession(&c, &request, Some(&proof), None, &mut replay, NOW + 91),
        Err(Denial::Proof)
    );
    verify_possession(&c, &request, Some(&proof), None, &mut replay, NOW + 90)?;
    c.cnf = Confirmation::Certificate("peer".into());
    assert_eq!(
        verify_possession(&c, &request, None, Some("other"), &mut replay, NOW),
        Err(Denial::Proof)
    );
    verify_possession(&c, &request, None, Some("peer"), &mut replay, NOW)?;
    Ok(())
}
#[test]
fn saturation_never_evicts_a_live_replay_fence() -> Result<(), Denial> {
    let mut replay = Replay::new(1)?;
    replay.admit("key", "one", NOW + 180, NOW)?;
    assert_eq!(
        replay.admit("key", "two", NOW + 180, NOW),
        Err(Denial::Capacity)
    );
    assert_eq!(
        replay.admit("key", "one", NOW + 180, NOW),
        Err(Denial::Replay)
    );
    assert_eq!(replay.counters.full, 1);
    replay.admit("key", "two", NOW + 360, NOW + 180)?;
    Ok(())
}
#[test]
fn shared_sdk_proof_fixture_matches_rust_construction() -> Result<(), Box<dyn std::error::Error>> {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../../../node/parity/auth-proof.json"))?;
    let token = fixture["token"].as_str().ok_or("missing token")?;
    let request = ProofRequest {
        method: "POST",
        target: "https://agindow.test/v1/guard",
        token,
    };
    assert_eq!(
        prove(&request, "parity-proof", NOW, &key())?,
        fixture["proof"].as_str().ok_or("proof")?
    );
    let claims = cache()?.validate(token, &context(), NOW)?;
    verify_possession(
        &claims,
        &request,
        fixture["proof"].as_str(),
        None,
        &mut Replay::new(8)?,
        NOW,
    )?;
    Ok(())
}
