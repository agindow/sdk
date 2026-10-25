use agw_stats::control::{Decision, Error, Evidence, Floors, Override, Signal};

fn evidence(signal: Signal, effect: f64, p_value: f64) -> Evidence {
    Evidence {
        signal,
        effect,
        p_value,
        tests: 10,
    }
}
#[test]
fn inclusive_floors_and_bonferroni_significance_are_independent() -> Result<(), Error> {
    let floors = Floors::new(0.05, 0.1, 0.4, &[])?;
    assert_eq!(
        floors.evaluate("t", "/x", evidence(Signal::MeanShift, 0.399, 0.0))?,
        Decision::BelowMateriality
    );
    assert_eq!(
        floors.evaluate("t", "/x", evidence(Signal::EnumNovelty, 0.1, 0.005))?,
        Decision::Eligible { score: 0.2 }
    );
    assert_eq!(
        floors.evaluate("t", "/x", evidence(Signal::EnumNovelty, 1.0, 0.0051))?,
        Decision::NotSignificant
    );
    Ok(())
}
#[test]
fn overrides_are_exact_customer_field_and_signal_scoped() -> Result<(), Error> {
    let overrides = [Override {
        tenant: "t",
        field: "/x",
        signal: Signal::EnumNovelty,
        floor: 0.8,
    }];
    let floors = Floors::new(0.05, 0.1, 0.4, &overrides)?;
    let e = evidence(Signal::EnumNovelty, 0.5, 0.0);
    assert_eq!(floors.evaluate("t", "/x", e)?, Decision::BelowMateriality);
    for (tenant, field) in [("other", "/x"), ("t", "/y")] {
        assert!(floors.evaluate(tenant, field, e)?.eligible());
    }
    assert!(
        floors
            .evaluate("t", "/x", evidence(Signal::Completeness, 0.5, 0.0))?
            .eligible()
    );
    assert!(Floors::new(0.05, 0.1, 0.4, &[overrides[0], overrides[0]]).is_err());
    Ok(())
}
#[test]
fn all_agent_signals_outweigh_equal_mean_movement() -> Result<(), Error> {
    let floors = Floors::new(0.05, 0.1, 0.1, &[])?;
    for signal in [
        Signal::EnumNovelty,
        Signal::UnitHypothesis,
        Signal::Completeness,
        Signal::TypeConsistency,
        Signal::StringFormat,
    ] {
        assert_eq!(
            floors.evaluate("t", "/x", evidence(signal, 0.5, 0.0))?,
            Decision::Eligible { score: 1.0 }
        );
    }
    assert_eq!(
        floors.evaluate("t", "/x", evidence(Signal::MeanShift, 0.5, 0.0))?,
        Decision::Eligible { score: 0.5 }
    );
    Ok(())
}
#[test]
fn rejects_non_finite_invalid_or_unbounded_evidence_and_configuration() -> Result<(), Error> {
    let floors = Floors::new(0.05, 0.1, 0.4, &[])?;
    for value in [f64::NAN, f64::INFINITY, -0.1, 1.1] {
        assert!(
            floors
                .evaluate("t", "/x", evidence(Signal::MeanShift, value, 0.0))
                .is_err()
        );
        assert!(
            floors
                .evaluate("t", "/x", evidence(Signal::MeanShift, 1.0, value))
                .is_err()
        );
        assert!(Floors::new(0.05, value, 0.4, &[]).is_err());
    }
    for alpha in [0.0, 1.0, f64::NAN] {
        assert!(Floors::new(alpha, 0.1, 0.4, &[]).is_err());
    }
    assert!(Floors::new(0.05, 0.0, 0.4, &[]).is_err());
    for tests in [0, 4097] {
        let mut e = evidence(Signal::MeanShift, 1.0, 0.0);
        e.tests = tests;
        assert!(floors.evaluate("t", "/x", e).is_err());
    }
    Ok(())
}
#[test]
fn ranking_orders_weighted_scores_then_agent_ties_and_omits_suppressed() -> Result<(), Error> {
    let floors = Floors::new(0.05, 0.1, 0.1, &[])?;
    let items = [
        evidence(Signal::MeanShift, 0.5, 0.0),
        evidence(Signal::EnumNovelty, 0.25, 0.0),
        evidence(Signal::Completeness, 0.8, 0.0),
        evidence(Signal::UnitHypothesis, 0.9, 0.1),
    ];
    let ranked = floors.rank("t", "/x", &items)?;
    assert_eq!(
        ranked.iter().map(|r| r.index).collect::<Vec<_>>(),
        vec![2, 1, 0]
    );
    assert_eq!(ranked[0].score, 1.6);
    assert_eq!(
        floors.rank("t", "/x", &vec![items[0]; 4097]),
        Err(Error::ResourceLimit)
    );
    Ok(())
}
