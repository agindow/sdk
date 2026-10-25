use agw_diff::{DriftClass, Side};
use agw_stats::unit::{Conversion, Detection, Error, detect, measure};

fn drift(before: &[f64], after: &[f64]) -> agw_stats::unit::Finding {
    match detect("amount", before, after) {
        Ok(Detection::Drift(finding)) => finding,
        other => panic!("expected drift: {other:?}"),
    }
}
fn near(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-10, "{actual} != {expected}");
}

#[test]
fn empirical_metrics_match_independent_unequal_window_calculation() -> Result<(), Error> {
    let m = measure(&[0., 2.], &[1., 1., 1.])?;
    near(m.wasserstein, 1.);
    near(m.normalized_wasserstein, 1.);
    // Shared bins: underflow, <=0, <=2, overflow. Floor then normalize.
    let p = [
        1e-6 / 1.000002,
        0.5 / 1.000002,
        0.5 / 1.000002,
        1e-6 / 1.000002,
    ];
    let q = [
        1e-6 / 1.000003,
        1e-6 / 1.000003,
        1. / 1.000003,
        1e-6 / 1.000003,
    ];
    let expected: f64 = p
        .into_iter()
        .zip(q)
        .map(|(p, q): (f64, f64)| (p - q) * (p / q).ln())
        .sum();
    near(m.psi, expected);
    near(measure(&[1., 1., 1.], &[0., 2.])?.wasserstein, 1.);
    Ok(())
}

#[test]
fn diagnoses_all_four_known_factors() {
    for (ratio, conversion) in [
        (100., Conversion::Times100),
        (0.01, Conversion::Divide100),
        (1000., Conversion::Times1000),
        (0.001, Conversion::Divide1000),
    ] {
        let f = drift(&[3499.; 20], &[3499. * ratio; 20]);
        assert_eq!(f.class(), DriftClass::Semantic);
        let h = f
            .hypothesis()
            .unwrap_or_else(|| panic!("expected diagnosis"));
        assert_eq!(h.conversion, conversion);
        near(h.ratio, ratio);
        near(h.tolerance, 0.02);
        near(h.baseline_median, 3499.);
        near(h.observed_median, 3499. * ratio);
    }
}

#[test]
fn tolerance_is_relative_and_inclusive() {
    for factor in [100., 0.01, 1000., 0.001] {
        for ratio in [factor * 0.98, factor * 1.02] {
            assert!(
                drift(&[1.; 20], &[ratio; 20]).hypothesis().is_some(),
                "boundary {ratio}"
            );
        }
    }
    for ratio in [97.999, 102.001, 2., -100.] {
        assert!(drift(&[1.; 20], &[ratio; 20]).hypothesis().is_none());
    }
}

#[test]
fn ordinary_growth_and_one_outlier_do_not_meet_both_gates() -> Result<(), Error> {
    assert!(matches!(
        detect("x", &[100.; 20], &[110.; 20])?,
        Detection::Stable(_)
    ));
    let mut after = [100.; 100];
    after[0] = 10000.;
    assert!(matches!(
        detect("x", &[100.; 100], &after)?,
        Detection::Stable(_)
    ));
    assert!(matches!(
        detect("x", &[100.; 20], &[100.; 20])?,
        Detection::Stable(_)
    ));
    // Normalized magnitude gate passes, but PSI remains below 0.2.
    let mut baseline = [1.; 100];
    baseline[99] = 100.;
    let mut observed = baseline;
    observed[99] = 10000.;
    let m = measure(&baseline, &observed)?;
    assert!(m.normalized_wasserstein >= 0.2 && m.psi < 0.2);
    assert!(matches!(
        detect("x", &baseline, &observed)?,
        Detection::Stable(_)
    ));
    Ok(())
}

#[test]
fn insufficient_windows_are_not_clean_and_all_inputs_are_validated() -> Result<(), Error> {
    for n in [0, 1, 19] {
        assert!(matches!(
            detect("x", &vec![1.; n], &[100.; 20])?,
            Detection::InsufficientEvidence
        ));
    }
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(detect("x", &[], &[bad]).err(), Some(Error::InvalidInput));
        assert_eq!(measure(&[1.], &[bad]).err(), Some(Error::InvalidInput));
    }
    assert_eq!(
        detect("", &[1.; 20], &[2.; 20]).err(),
        Some(Error::InvalidInput)
    );
    assert_eq!(
        detect(&"x".repeat(4097), &[], &[]).err(),
        Some(Error::ResourceLimit)
    );
    assert_eq!(
        detect("x", &[1.; 4097], &[]).err(),
        Some(Error::ResourceLimit)
    );
    assert_eq!(measure(&[], &[1.]).err(), Some(Error::InvalidInput));
    assert_eq!(
        detect("x", &[f64::MIN_POSITIVE; 20], &[f64::MAX; 20]).err(),
        Some(Error::NumericRange)
    );
    Ok(())
}

#[test]
fn zero_negative_and_extreme_values_do_not_invent_units() -> Result<(), Error> {
    assert!(drift(&[0.; 20], &[1.; 20]).hypothesis().is_none());
    assert!(matches!(
        detect("x", &[0.; 20], &[0.; 20])?,
        Detection::Stable(_)
    ));
    assert!(matches!(
        detect("x", &[-1.; 20], &[1.; 20])?,
        Detection::Stable(_)
    ));
    assert!(drift(&[-1.; 20], &[-100.; 20]).hypothesis().is_some());
    let m = measure(&[f64::MAX; 20], &[f64::MAX; 20])?;
    near(m.wasserstein, 0.);
    let before = [2., 4.].repeat(10);
    let after = [200., 400.].repeat(10);
    near(
        drift(&before, &after)
            .hypothesis()
            .unwrap_or_else(|| panic!("expected diagnosis"))
            .baseline_median,
        3.,
    );
    assert_eq!(before, [2., 4.].repeat(10));
    Ok(())
}

#[test]
fn audit_contains_diagnosis_and_clears_stale_repair_claims()
-> Result<(), Box<dyn std::error::Error>> {
    use agw_audit::contracts::audit_artifact::AuditArtifact;
    let fixture = include_str!("../../../../../contracts/examples/audit_artifact.valid.json");
    let mut v: serde_json::Value = serde_json::from_str(fixture)?;
    v["decision"] = "blocked".into();
    v["hypothesis"] = "stale".into();
    v["detail"] = "stale".into();
    v["confidence"] = 1.0.into();
    v["note"] = "stale".into();
    let draft: AuditArtifact = serde_json::from_value(v)?;
    let finding = drift(&[3499.; 20], &[34.99; 20]);
    let audit = finding.audit(draft.clone(), Side::Response)?;
    let json = serde_json::to_value(&audit)?;
    assert_eq!(json["hypothesis"], "unit_conversion");
    assert_eq!(
        json["detail"],
        "amount: median 3499 → 34.99, ratio 0.01 (±2%)"
    );
    assert_eq!(json["drift"]["class"], "semantic");
    assert_eq!(json["drift"]["side"], "response");
    assert_eq!(json["drift"]["changes"][0]["path"], "amount");
    assert_eq!(json["decision"], "blocked");
    assert!(json["patch"].is_null());
    assert_eq!(json["proof"]["write_integrity"], false);
    for field in ["note", "confidence"] {
        assert!(json.get(field).is_none());
    }
    let _: AuditArtifact = serde_json::from_value(json)?;
    let unnamed = drift(&[1.; 20], &[2.; 20]).audit(draft, Side::Request)?;
    let json = serde_json::to_value(unnamed)?;
    for field in ["hypothesis", "detail"] {
        assert!(json.get(field).is_none());
    }
    assert_eq!(json["drift"]["side"], "request");
    Ok(())
}
