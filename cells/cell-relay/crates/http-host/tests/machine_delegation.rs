#[path = "support/identity.rs"]
mod identity;
use agw_http_host::machine::{DelegateRequest, Denial, Exchange, ProofRequest, Replay, prove};
use identity::*;
#[test]
fn delegation_attenuates_and_requires_parents_possession() -> Result<(), Denial> {
    let parent = token(&claims())?;
    let mut keys = cache()?;
    let mut exchange = Exchange::new(
        context().issuer.into(),
        "one".into(),
        key(),
        8,
        "fixture-instance".into(),
    )?;
    let mut replay = Replay::new(16)?;
    let proof_req = ProofRequest {
        method: "POST",
        target: "https://agindow.test/v1/token",
        token: &parent,
    };
    let proof = prove(&proof_req, "delegate-one", NOW, &key())?;
    let req = DelegateRequest {
        parent: &parent,
        context: context(),
        proof: Some(&proof),
        tls_peer: None,
        target: proof_req.target,
        tools: vec!["read".into()],
        exp: NOW + 100,
        subject: "child",
        cnf: claims().cnf,
        max_depth: 3,
    };
    let child = exchange.delegate(&req, &mut keys, &mut replay, NOW)?;
    let c = keys.validate(&child, &context(), NOW)?;
    assert_eq!(c.act.len(), 1);
    assert_eq!(c.tools, vec!["read"]);
    assert_eq!(c.act[0].jti, claims().jti);
    let mut wrong = context();
    wrong.tool = "write";
    assert_eq!(
        keys.validate(&child, &wrong, NOW),
        Err(Denial::Delegation(1))
    );
    assert_eq!(
        exchange.delegate(&req, &mut keys, &mut replay, NOW),
        Err(Denial::Replay)
    );
    Ok(())
}
#[test]
fn widening_expiry_and_missing_proof_are_rejected() -> Result<(), Denial> {
    let parent = token(&claims())?;
    let mut keys = cache()?;
    let mut exchange = Exchange::new(
        context().issuer.into(),
        "one".into(),
        key(),
        8,
        "fixture-instance".into(),
    )?;
    let mut replay = Replay::new(8)?;
    for (tools, exp) in [
        (vec!["delete".into()], NOW + 100),
        (vec!["read".into()], NOW + 301),
    ] {
        let req = DelegateRequest {
            parent: &parent,
            context: context(),
            proof: None,
            tls_peer: None,
            target: "https://agindow.test/v1/token",
            tools,
            exp,
            subject: "child",
            cnf: claims().cnf,
            max_depth: 3,
        };
        assert!(
            exchange
                .delegate(&req, &mut keys, &mut replay, NOW)
                .is_err()
        );
    }
    Ok(())
}
#[test]
fn chains_intersect_to_depth_three_and_expiry_cascades() -> Result<(), Denial> {
    let mut parent = token(&claims())?;
    let mut keys = cache()?;
    let mut exchange = Exchange::new(
        context().issuer.into(),
        "one".into(),
        key(),
        8,
        "chain-instance".into(),
    )?;
    let mut replay = Replay::new(16)?;
    for depth in 0..3 {
        let target = "https://agindow.test/v1/token";
        let proof = prove(
            &ProofRequest {
                method: "POST",
                target,
                token: &parent,
            },
            &format!("proof-{depth}"),
            NOW,
            &key(),
        )?;
        parent = exchange.delegate(
            &DelegateRequest {
                parent: &parent,
                context: context(),
                proof: Some(&proof),
                tls_peer: None,
                target,
                tools: vec!["read".into()],
                exp: NOW + 100 - depth,
                subject: "child",
                cnf: claims().cnf,
                max_depth: 3,
            },
            &mut keys,
            &mut replay,
            NOW,
        )?;
        assert_eq!(
            keys.validate(&parent, &context(), NOW)?.act.len(),
            (depth + 1) as usize
        );
    }
    let proof = prove(
        &ProofRequest {
            method: "POST",
            target: "https://agindow.test/v1/token",
            token: &parent,
        },
        "too-deep",
        NOW,
        &key(),
    )?;
    let req = DelegateRequest {
        parent: &parent,
        context: context(),
        proof: Some(&proof),
        tls_peer: None,
        target: "https://agindow.test/v1/token",
        tools: vec!["read".into()],
        exp: NOW + 90,
        subject: "child",
        cnf: claims().cnf,
        max_depth: 3,
    };
    assert_eq!(
        exchange.delegate(&req, &mut keys, &mut replay, NOW),
        Err(Denial::Capacity)
    );
    assert_eq!(
        keys.validate(&parent, &context(), NOW + 128),
        Err(Denial::Expiry)
    );
    let mut forged = claims();
    forged.act.push(agw_http_host::machine::Actor {
        jti: "parent".into(),
        sub: "root".into(),
        tenant: "b".into(),
        environment: "test".into(),
        aud: forged.aud.clone(),
        connection: "c".into(),
        tools: forged.tools.clone(),
        exp: forged.exp,
    });
    assert_eq!(
        keys.validate(&token(&forged)?, &context(), NOW),
        Err(Denial::Tenant)
    );
    Ok(())
}
#[test]
fn ancestor_scope_denial_names_the_excluding_link() -> Result<(), Denial> {
    let mut c = claims();
    c.act.push(agw_http_host::machine::Actor {
        jti: "ancestor".into(),
        sub: "root".into(),
        tenant: c.tenant.clone(),
        environment: c.environment.clone(),
        aud: c.aud.clone(),
        connection: c.connection.clone(),
        tools: vec!["read".into()],
        exp: c.exp,
    });
    assert_eq!(
        cache()?.validate(&token(&c)?, &context(), NOW),
        Err(Denial::Delegation(0))
    );
    let parent = token(&claims())?;
    let proof = prove(
        &ProofRequest {
            method: "POST",
            target: "https://agindow.test/v1/token",
            token: &parent,
        },
        "narrowed",
        NOW,
        &key(),
    )?;
    let mut exchange = Exchange::new(
        context().issuer.into(),
        "one".into(),
        key(),
        8,
        "denial-instance".into(),
    )?;
    let child = exchange.delegate(
        &DelegateRequest {
            parent: &parent,
            context: context(),
            proof: Some(&proof),
            tls_peer: None,
            target: "https://agindow.test/v1/token",
            tools: vec!["read".into()],
            exp: NOW + 100,
            subject: "child",
            cnf: claims().cnf,
            max_depth: 3,
        },
        &mut cache()?,
        &mut Replay::new(8)?,
        NOW,
    )?;
    let mut ctx = context();
    ctx.tool = "write";
    assert_eq!(
        cache()?.validate(&child, &ctx, NOW),
        Err(Denial::Delegation(1))
    );
    Ok(())
}
