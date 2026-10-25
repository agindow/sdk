use agw_invariants::learning::{Profile, derive};
use agw_invariants::store::{Controls, Limits, Scope, Store};
use agw_invariants::{Form, Relation, Rule, Scalar, Verdict, compile};
fn rule(form: Form) -> Rule {
    Rule {
        id: "r".into(),
        form,
    }
}

#[test]
fn equality_edges_and_unavailable_never_become_passes() -> Result<(), Box<dyn std::error::Error>> {
    const VALUES: [Scalar<'static>; 2] = [Scalar::Integer(1), Scalar::Integer(1)];
    for (relation, expected) in [
        (Relation::Equal, Verdict::Pass),
        (Relation::Less, Verdict::Violation),
        (Relation::LessEqual, Verdict::Pass),
        (Relation::Greater, Verdict::Violation),
        (Relation::GreaterEqual, Verdict::Pass),
    ] {
        let p = compile(
            &[rule(Form::Relation {
                left: "a".into(),
                right: "b".into(),
                relation,
            })],
            &["a", "b"],
        )?;
        assert_eq!(p.rules()[0].evaluate(&VALUES), expected);
        assert_eq!(
            p.rules()[0].evaluate(&[Scalar::Integer(1)]),
            Verdict::Unavailable
        );
        assert_eq!(
            p.rules()[0].evaluate(&[Scalar::Text("1"), Scalar::Integer(1)]),
            Verdict::Unavailable
        );
    }
    let p = compile(
        &[rule(Form::CrossEndpoint {
            left: "a".into(),
            right: "b".into(),
            tolerance: u64::MAX,
        })],
        &["a", "b"],
    )?;
    assert_eq!(
        p.rules()[0].evaluate(&[Scalar::Integer(i64::MIN), Scalar::Integer(i64::MAX)]),
        Verdict::Pass
    );
    let range = compile(
        &[rule(Form::Range {
            field: "a".into(),
            min: 0,
            max: 1,
        })],
        &["a"],
    )?;
    assert_eq!(
        range.rules()[0].evaluate(&[Scalar::Integer(0)]),
        Verdict::Pass
    );
    assert_eq!(
        range.rules()[0].evaluate(&[Scalar::Integer(2)]),
        Verdict::Violation
    );
    Ok(())
}

#[test]
fn definition_and_vocabulary_limits_are_enforced() {
    let definitions = [
        Form::Enum {
            field: "a".into(),
            values: vec!["x".into(), "x".into()],
        },
        Form::Enum {
            field: "a".into(),
            values: vec!["x".repeat(257)],
        },
        Form::Enum {
            field: "a".into(),
            values: vec![],
        },
        Form::Sum {
            total: "a".into(),
            terms: vec!["a".into(); 33],
            tolerance: 0,
        },
    ];
    for form in definitions {
        assert!(compile(&[rule(form)], &["a"]).is_err());
    }
    let r = rule(Form::Range {
        field: "a".into(),
        min: 0,
        max: 1,
    });
    assert!(compile(&[r.clone(), r.clone()], &["a"]).is_err());
    assert!(compile(&[Rule { id: "".into(), ..r }], &["a"]).is_err());
    assert!(compile(&[], &vec!["a"; 129]).is_err());
    assert!(
        derive(&[Profile::Vocabulary {
            field: "a",
            samples: 2,
            complete: true,
            values: &["x", "x"]
        }])
        .is_err()
    );
}

#[test]
fn version_and_control_conflicts_are_atomic() -> Result<(), Box<dyn std::error::Error>> {
    let key = Scope {
        vendor: "v".into(),
        operation: "op".into(),
        contract_version: 1,
    };
    let r = rule(Form::Range {
        field: "a".into(),
        min: 0,
        max: 1,
    });
    let mut store = Store::new(Limits::default())?;
    assert!(
        store
            .publish(key.clone(), 0, std::slice::from_ref(&r), &["a"])
            .is_err()
    );
    store.publish(key.clone(), 2, std::slice::from_ref(&r), &["a"])?;
    assert!(
        store
            .publish(key.clone(), 1, std::slice::from_ref(&r), &["a"])
            .is_err()
    );
    assert!(
        store
            .select(
                &key,
                2,
                &Controls {
                    pin: Some(1),
                    ..Controls::default()
                }
            )
            .is_err()
    );
    for controls in [
        Controls {
            disabled: vec!["r".into(), "r".into()],
            ..Controls::default()
        },
        Controls {
            overrides: vec![r.clone(), r.clone()],
            ..Controls::default()
        },
        Controls {
            overrides: vec![r.clone()],
            disabled: vec!["r".into()],
            ..Controls::default()
        },
        Controls {
            overrides: vec![Rule {
                id: "other".into(),
                ..r
            }],
            ..Controls::default()
        },
    ] {
        assert!(store.select(&key, 2, &controls).is_err());
    }
    assert_eq!(
        store.select(&key, 2, &Controls::default())?.program.rules()[0].id(),
        "r"
    );
    Ok(())
}
