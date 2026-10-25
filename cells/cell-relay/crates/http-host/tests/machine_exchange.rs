#[path = "support/identity.rs"]
mod identity;
use agw_http_host::machine::{
    Denial, Exchange, ExchangeRequest, Registration, SubjectClaims, sign_subject,
};
use identity::*;
fn registration() -> Registration {
    Registration {
        id: "github-a".into(),
        kid: "one".into(),
        audiences: vec!["agindow:guard".into()],
        issuer: "https://oidc.test".into(),
        subject: "repo:acme/agents:main".into(),
        audience: "agindow".into(),
        tenant: "a".into(),
        environment: "test".into(),
        connection: "c".into(),
        tools: vec!["read".into(), "write".into()],
        current: true,
        private_key_jwt: false,
    }
}
fn subject() -> SubjectClaims {
    SubjectClaims {
        iss: "https://oidc.test".into(),
        sub: "repo:acme/agents:main".into(),
        aud: "agindow".into(),
        iat: NOW,
        nbf: NOW,
        exp: NOW + 120,
        jti: Some("assertion-one".into()),
    }
}
fn request(assertion: &str) -> ExchangeRequest<'_> {
    ExchangeRequest {
        registration: "github-a",
        assertion,
        audience: "agindow:guard",
        tools: vec!["read".into()],
        ttl: 120,
        cnf: claims().cnf,
        subject: "workload",
    }
}
#[test]
fn registered_identity_is_only_authority_and_scope_cannot_widen() -> Result<(), Denial> {
    let mut keys = cache()?;
    keys.insert(
        agw_http_host::machine::PublicKey {
            issuer: "https://oidc.test".into(),
            kid: "one".into(),
            key: key().verifying_key().into(),
            until: NOW + 300,
        },
        NOW,
    )?;
    let mut exchange = Exchange::new(
        "https://agindow.test".into(),
        "one".into(),
        key(),
        8,
        "fixture-instance".into(),
    )?;
    exchange.register(registration())?;
    let assertion = sign_subject(&subject(), "one", &key())?;
    let token = exchange.exchange(&request(&assertion), &mut keys, NOW)?;
    assert_eq!(keys.validate(&token, &context(), NOW)?.tools, vec!["read"]);
    let mut req = request(&assertion);
    req.tools.push("delete".into());
    assert_eq!(exchange.exchange(&req, &mut keys, NOW), Err(Denial::Scope));
    let mut r = registration();
    r.tools = vec!["write".into()];
    exchange.register(r)?;
    assert_eq!(
        exchange.exchange(&request(&assertion), &mut keys, NOW),
        Err(Denial::Scope)
    );
    exchange.remove("github-a");
    assert_eq!(
        exchange.exchange(&request(&assertion), &mut keys, NOW),
        Err(Denial::Registration)
    );
    Ok(())
}
#[test]
fn oidc_defect_matrix_and_cross_tenant_subject_confusion() -> Result<(), Denial> {
    let mut keys = cache()?;
    keys.insert(
        agw_http_host::machine::PublicKey {
            issuer: "https://oidc.test".into(),
            kid: "one".into(),
            key: key().verifying_key().into(),
            until: NOW + 300,
        },
        NOW,
    )?;
    let mut exchange = Exchange::new(
        context().issuer.into(),
        "one".into(),
        key(),
        8,
        "fixture-instance".into(),
    )?;
    exchange.register(registration())?;
    let mut b = registration();
    b.id = "tenant-b".into();
    b.tenant = "b".into();
    b.subject = "repo:other/agents:main".into();
    exchange.register(b)?;
    let assertion = sign_subject(&subject(), "one", &key())?;
    let mut req = request(&assertion);
    req.registration = "tenant-b";
    assert_eq!(
        exchange.exchange(&req, &mut keys, NOW),
        Err(Denial::Registration)
    );
    for change in [
        |c: &mut SubjectClaims| c.iss = "unknown".into(),
        |c: &mut SubjectClaims| c.sub = "repo:evil/agents:main".into(),
        |c: &mut SubjectClaims| c.aud = "other".into(),
        |c: &mut SubjectClaims| c.exp = NOW - 30,
    ] {
        let mut c = subject();
        change(&mut c);
        let a = sign_subject(&c, "one", &key())?;
        assert!(exchange.exchange(&request(&a), &mut keys, NOW).is_err());
    }
    Ok(())
}
#[test]
fn spiffe_and_private_key_assertions_use_same_exchange_with_replay_protection() -> Result<(), Denial>
{
    let mut keys = cache()?;
    let mut exchange = Exchange::new(
        context().issuer.into(),
        "one".into(),
        key(),
        8,
        "fixture-instance".into(),
    )?;
    {
        let (id, sub, private) = ("client", "client", true);
        let mut r = registration();
        r.id = id.into();
        r.issuer = if private {
            "client"
        } else {
            "https://spiffe.test"
        }
        .into();
        r.subject = sub.into();
        r.private_key_jwt = private;
        keys.insert(
            agw_http_host::machine::PublicKey {
                issuer: r.issuer.clone(),
                kid: "one".into(),
                key: key().verifying_key().into(),
                until: NOW + 300,
            },
            NOW,
        )?;
        let c = SubjectClaims {
            iss: r.issuer.clone(),
            sub: sub.into(),
            ..subject()
        };
        exchange.register(r)?;
        let a = sign_subject(&c, "one", &key())?;
        let mut req = request(&a);
        req.registration = id;
        assert!(exchange.exchange(&req, &mut keys, NOW).is_ok());
        if private {
            assert_eq!(exchange.exchange(&req, &mut keys, NOW), Err(Denial::Replay));
        }
    }
    Ok(())
}
#[test]
fn issuer_rotation_current_key_revocation_and_exchange_rate_are_bounded() -> Result<(), Denial> {
    let mut keys = cache()?;
    keys.insert(
        agw_http_host::machine::PublicKey {
            issuer: "https://oidc.test".into(),
            kid: "one".into(),
            key: key().verifying_key().into(),
            until: NOW + 300,
        },
        NOW,
    )?;
    let mut exchange = Exchange::new(
        context().issuer.into(),
        "one".into(),
        key(),
        8,
        "rotation-instance".into(),
    )?;
    let mut r = registration();
    r.kid = "two".into();
    exchange.register(r)?;
    let a = sign_subject(&subject(), "one", &key())?;
    assert_eq!(
        exchange.exchange(&request(&a), &mut keys, NOW),
        Err(Denial::Signature)
    );
    keys.insert(
        agw_http_host::machine::PublicKey {
            issuer: "https://oidc.test".into(),
            kid: "two".into(),
            key: key().verifying_key().into(),
            until: NOW + 300,
        },
        NOW,
    )?;
    let a = sign_subject(&subject(), "two", &key())?;
    for _ in 0..60 {
        assert!(exchange.exchange(&request(&a), &mut keys, NOW).is_ok());
    }
    assert_eq!(
        exchange.exchange(&request(&a), &mut keys, NOW),
        Err(Denial::Limited)
    );
    assert_eq!(exchange.limited, 1);
    let mut r = registration();
    r.kid = "two".into();
    r.current = false;
    exchange.register(r)?;
    assert_eq!(
        exchange.exchange(&request(&a), &mut keys, NOW + 60),
        Err(Denial::Registration)
    );
    Ok(())
}
#[test]
fn real_jwt_svid_profile_accepts_es256_with_only_standard_required_claims()
-> Result<(), Box<dyn std::error::Error>> {
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD as B64};
    use p256::ecdsa::{Signature, SigningKey, signature::Signer};
    let key = SigningKey::from_bytes((&[11u8; 32]).into())?;
    let mut keys = cache()?;
    keys.insert(
        agw_http_host::machine::PublicKey {
            issuer: "spiffe://example.test".into(),
            kid: "svid".into(),
            key: (*key.verifying_key()).into(),
            until: NOW + 300,
        },
        NOW,
    )?;
    let mut r = registration();
    r.issuer = "spiffe://example.test".into();
    r.subject = "spiffe://example.test/workload".into();
    r.kid = "svid".into();
    let mut exchange = Exchange::new(
        context().issuer.into(),
        "one".into(),
        identity::key(),
        8,
        "svid-instance".into(),
    )?;
    exchange.register(r)?;
    let body =
        serde_json::json!({"sub":"spiffe://example.test/workload","aud":["agindow"],"exp":NOW+120});
    let signed = format!(
        "{}.{}",
        B64.encode(br#"{"alg":"ES256","kid":"svid"}"#),
        B64.encode(serde_json::to_vec(&body)?)
    );
    let signature: Signature = key.sign(signed.as_bytes());
    let assertion = format!("{signed}.{}", B64.encode(signature.to_bytes()));
    let token = exchange.exchange(&request(&assertion), &mut keys, NOW)?;
    assert_eq!(keys.validate(&token, &context(), NOW)?.tenant, "a");
    Ok(())
}
