#[path = "support/identity.rs"]
mod identity;
use agw_http_host::machine::{Denial, PublicKey};
use identity::*;
#[test]
fn validates_exact_scope_without_io_and_rejects_each_claim_defect() -> Result<(), Denial> {
    let mut cache = cache()?;
    assert_eq!(
        cache.validate(&token(&claims())?, &context(), NOW)?.tenant,
        "a"
    );
    type Defect = (fn(&mut agw_http_host::machine::Claims), Denial);
    let changes: [Defect; 7] = [
        (|c| c.exp = NOW - 30, Denial::Expiry),
        (|c| c.aud = "agindow:mcp".into(), Denial::Audience),
        (|c| c.tenant = "b".into(), Denial::Tenant),
        (|c| c.environment = "live".into(), Denial::Tenant),
        (|c| c.connection = "other".into(), Denial::Scope),
        (|c| c.tools = vec!["delete".into()], Denial::Scope),
        (|c| c.nbf = NOW + 31, Denial::Expiry),
    ];
    for (change, reason) in changes {
        let mut c = claims();
        change(&mut c);
        assert_eq!(cache.validate(&token(&c)?, &context(), NOW), Err(reason));
    }
    let mut wrong = token(&claims())?;
    wrong.push('a');
    assert!(matches!(
        cache.validate(&wrong, &context(), NOW),
        Err(Denial::Signature | Denial::Malformed)
    ));
    assert_eq!(
        cache.validate("fixture-secret", &context(), NOW),
        Err(Denial::Malformed)
    );
    Ok(())
}
#[test]
fn bounds_skew_key_rotation_and_cache_staleness() -> Result<(), Denial> {
    let mut cache = cache()?;
    let t = token(&claims())?;
    assert!(cache.validate(&t, &context(), NOW - 30).is_ok());
    assert_eq!(
        cache.validate(&t, &context(), NOW - 31),
        Err(Denial::Expiry)
    );
    assert_eq!(
        cache.validate(&t, &context(), NOW + 300),
        Err(Denial::Unavailable)
    );
    assert_eq!(cache.counters.stale, 1);
    for kid in ["two", "three"] {
        cache.insert(
            PublicKey {
                issuer: context().issuer.into(),
                kid: kid.into(),
                key: key().verifying_key().into(),
                until: NOW + 200,
            },
            NOW,
        )?;
    }
    assert_eq!(cache.len(), 2);
    assert_eq!(cache.counters.dropped, 1);
    assert_eq!(
        cache.validate(&t, &context(), NOW),
        Err(Denial::Unavailable)
    );
    Ok(())
}
#[test]
fn no_credential_bytes_in_denial_output() -> Result<(), Denial> {
    let secret = token(&claims())?;
    let mut cache = cache()?;
    let mut ctx = context();
    ctx.tenant = "b";
    let output = format!("{:?}", cache.validate(&secret, &ctx, NOW));
    assert!(!output.contains(&secret));
    assert!(!output.contains("fixture-secret"));
    Ok(())
}
#[test]
fn expiry_skew_is_identical_at_both_edges() -> Result<(), Denial> {
    let mut cache = cache()?;
    cache.insert(
        PublicKey {
            issuer: context().issuer.into(),
            kid: "one".into(),
            key: key().verifying_key().into(),
            until: NOW + 500,
        },
        NOW + 200,
    )?;
    let t = token(&claims())?;
    assert!(cache.validate(&t, &context(), NOW + 329).is_ok());
    assert_eq!(
        cache.validate(&t, &context(), NOW + 330),
        Err(Denial::Expiry)
    );
    Ok(())
}
