use agw_invariants::{Error, Form, Relation, Rule, Scalar, Verdict, compile};

fn rule(form: Form) -> Rule {
    Rule {
        id: "check".into(),
        form,
    }
}
fn eval(form: Form, values: &[Scalar<'_>]) -> Verdict {
    let compiled = compile(&[rule(form)], &["a", "b", "c"]);
    assert!(compiled.is_ok());
    compiled
        .map(|p| p.rules()[0].evaluate(values))
        .unwrap_or(Verdict::Unavailable)
}

#[test]
fn all_forms_have_distinct_outcomes() {
    let values = [Scalar::Integer(10), Scalar::Integer(7), Scalar::Integer(3)];
    for form in [
        Form::Relation {
            left: "a".into(),
            relation: Relation::GreaterEqual,
            right: "b".into(),
        },
        Form::Range {
            field: "a".into(),
            min: 0,
            max: 10,
        },
        Form::Ordering {
            earlier: "b".into(),
            later: "a".into(),
        },
        Form::Sum {
            total: "a".into(),
            terms: vec!["b".into(), "c".into()],
            tolerance: 0,
        },
        Form::CrossEndpoint {
            left: "a".into(),
            right: "b".into(),
            tolerance: 3,
        },
    ] {
        assert_eq!(eval(form, &values), Verdict::Pass);
    }
    let closure = Form::Enum {
        field: "a".into(),
        values: vec!["paid".into()],
    };
    assert_eq!(
        eval(closure.clone(), &[Scalar::Text("paid")]),
        Verdict::Pass
    );
    assert_eq!(
        eval(closure.clone(), &[Scalar::Text("new")]),
        Verdict::EnumAddition
    );
    assert_eq!(eval(closure, &values), Verdict::Unavailable);
}

#[test]
fn violations_tolerance_and_widened_arithmetic() {
    let sum = Form::Sum {
        total: "a".into(),
        terms: vec!["b".into(), "c".into()],
        tolerance: 1,
    };
    assert_eq!(
        eval(
            sum.clone(),
            &[Scalar::Integer(9), Scalar::Integer(7), Scalar::Integer(3)]
        ),
        Verdict::Pass
    );
    assert_eq!(
        eval(
            sum.clone(),
            &[Scalar::Integer(8), Scalar::Integer(7), Scalar::Integer(3)]
        ),
        Verdict::Violation
    );
    assert_eq!(
        eval(
            sum.clone(),
            &[
                Scalar::Integer(i64::MIN),
                Scalar::Integer(i64::MAX),
                Scalar::Integer(1)
            ]
        ),
        Verdict::Violation
    );
    assert_eq!(eval(sum.clone(), &[]), Verdict::Unavailable);
    assert_eq!(eval(sum, &[Scalar::Missing; 3]), Verdict::Unavailable);
    assert_eq!(
        eval(
            Form::Range {
                field: "a".into(),
                min: 0,
                max: 10
            },
            &[Scalar::Integer(-1)]
        ),
        Verdict::Violation
    );
    assert_eq!(
        eval(
            Form::Ordering {
                earlier: "a".into(),
                later: "b".into()
            },
            &[Scalar::Integer(1), Scalar::Integer(0)]
        ),
        Verdict::Violation
    );
}

#[test]
fn every_relation_and_cross_endpoint_tolerance() {
    for (relation, expected) in [
        (Relation::Equal, false),
        (Relation::Less, true),
        (Relation::LessEqual, true),
        (Relation::Greater, false),
        (Relation::GreaterEqual, false),
    ] {
        assert_eq!(
            eval(
                Form::Relation {
                    left: "a".into(),
                    relation,
                    right: "b".into()
                },
                &[Scalar::Integer(1), Scalar::Integer(2)]
            ),
            if expected {
                Verdict::Pass
            } else {
                Verdict::Violation
            }
        );
    }
    let cross = Form::CrossEndpoint {
        left: "a".into(),
        right: "b".into(),
        tolerance: 2,
    };
    assert_eq!(
        eval(cross.clone(), &[Scalar::Integer(5), Scalar::Integer(2)]),
        Verdict::Violation
    );
    assert_eq!(
        eval(cross, &[Scalar::Integer(4), Scalar::Integer(2)]),
        Verdict::Pass
    );
}

#[test]
fn compiler_rejects_ambiguous_invalid_and_unbounded_definitions() {
    assert!(matches!(compile(&[], &["a", "a"]), Err(Error::Invalid)));
    assert!(matches!(
        compile(
            &[rule(Form::Range {
                field: "missing".into(),
                min: 0,
                max: 1
            })],
            &["a"]
        ),
        Err(Error::Invalid)
    ));
    assert!(matches!(
        compile(
            &[rule(Form::Range {
                field: "a".into(),
                min: 2,
                max: 1
            })],
            &["a"]
        ),
        Err(Error::Invalid)
    ));
    assert!(
        compile(
            &[rule(Form::Sum {
                total: "a".into(),
                terms: vec![],
                tolerance: 0
            })],
            &["a"]
        )
        .is_err()
    );
    assert!(
        compile(
            &vec![
                rule(Form::Range {
                    field: "a".into(),
                    min: 0,
                    max: 1
                });
                129
            ],
            &["a"]
        )
        .is_err()
    );
}
