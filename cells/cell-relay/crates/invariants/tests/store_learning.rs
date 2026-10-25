use agw_invariants::learning::{Profile, derive};
use agw_invariants::store::{Controls, Limits, Scope, Store};
use agw_invariants::{Form, Rule, Scalar, Verdict};
fn scope(operation: &str, contract_version: u64) -> Scope {
    Scope {
        vendor: "vendor".into(),
        operation: operation.into(),
        contract_version,
    }
}
fn range(max: i64) -> Rule {
    Rule {
        id: "amount".into(),
        form: Form::Range {
            field: "amount".into(),
            min: 0,
            max,
        },
    }
}

#[test]
fn histories_are_immutable_scoped_and_explicitly_selected() -> Result<(), Box<dyn std::error::Error>>
{
    let mut store = Store::new(Limits {
        scopes: 2,
        versions: 2,
    })?;
    store.publish(scope("pay", 1), 1, &[range(10)], &["amount"])?;
    let first = store.select(&scope("pay", 1), 1, &Controls::default())?;
    store.publish(scope("pay", 1), 2, &[range(20)], &["amount"])?;
    assert_eq!(
        first.program.rules()[0].evaluate(&[Scalar::Integer(15)]),
        Verdict::Violation
    );
    assert!(
        store
            .publish(scope("pay", 1), 2, &[range(30)], &["amount"])
            .is_err()
    );
    assert!(
        store
            .publish(scope("pay", 1), 3, &[range(30)], &["amount"])
            .is_err()
    );
    assert!(
        store
            .select(&scope("other", 1), 1, &Controls::default())
            .is_err()
    );
    assert!(
        store
            .select(&scope("pay", 2), 1, &Controls::default())
            .is_err()
    );
    store.publish(scope("pay", 2), 1, &[range(30)], &["amount"])?;
    assert!(
        store
            .publish(scope("other", 1), 1, &[range(30)], &["amount"])
            .is_err()
    );
    Ok(())
}

#[test]
fn customer_controls_pin_override_disable_and_reject_unknown_ids()
-> Result<(), Box<dyn std::error::Error>> {
    let mut store = Store::new(Limits::default())?;
    let key = scope("pay", 1);
    store.publish(key.clone(), 1, &[range(10)], &["amount"])?;
    store.publish(key.clone(), 2, &[range(20)], &["amount"])?;
    let controls = Controls {
        pin: Some(1),
        overrides: vec![range(100)],
        disabled: vec![],
    };
    let selected = store.select(&key, 2, &controls)?;
    assert_eq!(selected.version, 1);
    assert_eq!(selected.controls, controls);
    assert_eq!(
        selected.program.rules()[0].evaluate(&[Scalar::Integer(90)]),
        Verdict::Pass
    );
    let disabled = Controls {
        disabled: vec!["amount".into()],
        ..Controls::default()
    };
    assert!(store.select(&key, 2, &disabled)?.program.rules().is_empty());
    let bad = Controls {
        disabled: vec!["unknown".into()],
        ..Controls::default()
    };
    assert!(store.select(&key, 2, &bad).is_err());
    let invalid = Controls {
        overrides: vec![Rule {
            id: "amount".into(),
            form: Form::Range {
                field: "missing".into(),
                min: 0,
                max: 1,
            },
        }],
        ..Controls::default()
    };
    assert!(store.select(&key, 2, &invalid).is_err());
    assert_eq!(
        store.select(&key, 1, &Controls::default())?.program.rules()[0]
            .evaluate(&[Scalar::Integer(90)]),
        Verdict::Violation
    );
    Ok(())
}

#[test]
fn rejected_publication_does_not_consume_capacity() -> Result<(), Box<dyn std::error::Error>> {
    let mut store = Store::new(Limits {
        scopes: 1,
        versions: 1,
    })?;
    assert!(
        store
            .publish(scope("pay", 1), 1, &[range(-1)], &["amount"])
            .is_err()
    );
    store.publish(scope("other", 1), 1, &[range(10)], &["amount"])?;
    assert!(
        Store::new(Limits {
            scopes: 0,
            versions: 1
        })
        .is_err()
    );
    Ok(())
}

#[test]
fn complete_profiles_derive_candidates_without_activating_them()
-> Result<(), Box<dyn std::error::Error>> {
    let profiles = [
        Profile::Numeric {
            field: "amount",
            samples: 5,
            complete: true,
            min: -2,
            max: 10,
        },
        Profile::Vocabulary {
            field: "status",
            samples: 5,
            complete: true,
            values: &["pending", "paid"],
        },
        Profile::Numeric {
            field: "partial",
            samples: 5,
            complete: false,
            min: 0,
            max: 5,
        },
    ];
    let candidates = derive(&profiles)?;
    assert_eq!(candidates.len(), 2);
    let program = agw_invariants::compile(&candidates, &["amount", "status"])?;
    assert_eq!(
        program.rules()[0].evaluate(&[Scalar::Integer(-2)]),
        Verdict::Pass
    );
    assert_eq!(
        program.rules()[1].evaluate(&[Scalar::Integer(0), Scalar::Text("new")]),
        Verdict::EnumAddition
    );
    assert!(
        derive(&[Profile::Numeric {
            field: "bad",
            samples: 2,
            complete: true,
            min: 10,
            max: 1
        }])
        .is_err()
    );
    assert!(derive(&vec![profiles[0].clone(); 129]).is_err());
    assert!(
        derive(&[Profile::Numeric {
            field: "empty",
            samples: 0,
            complete: true,
            min: 0,
            max: 0
        }])?
        .is_empty()
    );
    Ok(())
}
