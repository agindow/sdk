use agw_invariants::store::{Controls, Limits, Scope, Store};
use agw_invariants::{Expected, Form, Rule, Scalar, compile};
#[test]
fn computed_evidence_retains_exact_expected_sum_and_binding_order()
-> Result<(), Box<dyn std::error::Error>> {
    let rule = Rule {
        id: "gross".into(),
        form: Form::Sum {
            total: "/total".into(),
            terms: vec!["/amount".into(), "/tax".into()],
            tolerance: 1,
        },
    };
    let program = compile(std::slice::from_ref(&rule), &["/total", "/amount", "/tax"])?;
    let values = [
        Scalar::Integer(118),
        Scalar::Integer(100),
        Scalar::Integer(20),
    ];
    let evidence = program.rules()[0].evidence(&values).ok_or("evidence")?;
    assert_eq!(evidence.actual, Scalar::Integer(118));
    assert_eq!(
        evidence.expected,
        Expected::Numeric {
            value: 120,
            relation: agw_invariants::Relation::Equal,
            tolerance: 1
        }
    );
    assert!(program.rules()[0].evidence(&[]).is_none());
    let key = Scope {
        vendor: "v".into(),
        operation: "op".into(),
        contract_version: 1,
    };
    let mut store = Store::new(Limits::default())?;
    store.publish(key.clone(), 1, &[rule], &["/total", "/amount", "/tax"])?;
    assert_eq!(
        store.select(&key, 1, &Controls::default())?.bindings(),
        &["/total", "/amount", "/tax"]
    );
    Ok(())
}

#[test]
fn evidence_covers_range_relation_enum_and_exact_integer_extremes()
-> Result<(), Box<dyn std::error::Error>> {
    use agw_invariants::Relation;
    let rules = [
        Rule {
            id: "range".into(),
            form: Form::Range {
                field: "a".into(),
                min: -1,
                max: 2,
            },
        },
        Rule {
            id: "order".into(),
            form: Form::Ordering {
                earlier: "a".into(),
                later: "b".into(),
            },
        },
        Rule {
            id: "cross".into(),
            form: Form::CrossEndpoint {
                left: "a".into(),
                right: "b".into(),
                tolerance: u64::MAX,
            },
        },
        Rule {
            id: "sum".into(),
            form: Form::Sum {
                total: "a".into(),
                terms: vec!["b".into(); 32],
                tolerance: 0,
            },
        },
        Rule {
            id: "enum".into(),
            form: Form::Enum {
                field: "c".into(),
                values: vec!["paid".into()],
            },
        },
    ];
    let p = compile(&rules, &["a", "b", "c"])?;
    let slots = [
        Scalar::Integer(i64::MIN),
        Scalar::Integer(i64::MAX),
        Scalar::Text("new"),
    ];
    let expected = [
        Expected::Range { min: -1, max: 2 },
        Expected::Numeric {
            value: i128::from(i64::MAX),
            relation: Relation::LessEqual,
            tolerance: 0,
        },
        Expected::Numeric {
            value: i128::from(i64::MAX),
            relation: Relation::Equal,
            tolerance: u64::MAX,
        },
        Expected::Numeric {
            value: i128::from(i64::MAX) * 32,
            relation: Relation::Equal,
            tolerance: 0,
        },
    ];
    for (rule, expected) in p.rules().iter().zip(expected) {
        assert_eq!(rule.evidence(&slots).ok_or("evidence")?.expected, expected);
    }
    assert_eq!(
        p.rules()[4]
            .evidence(&slots)
            .ok_or("enum evidence")?
            .expected,
        Expected::Vocabulary(&["paid".into()])
    );
    assert!(
        p.rules()
            .iter()
            .all(|r| r.evidence(&[Scalar::Missing; 3]).is_none())
    );
    Ok(())
}
