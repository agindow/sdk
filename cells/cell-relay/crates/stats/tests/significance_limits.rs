use agw_inspector::{Number, Value};
use agw_stats::{control, enumeration::Observation, significance as sig};
use chrono::DateTime;

#[test]
fn rejects_sample_and_text_limits_even_for_insufficient_windows()
-> Result<(), Box<dyn std::error::Error>> {
    let at = DateTime::from_timestamp(0, 0).ok_or("timestamp")?;
    let huge = "x".repeat(4097);
    let long = Value::String(huge.clone());
    let raw = Value::Number(Number::Verbatim(huge.clone().into()));
    assert_eq!(
        sig::enumeration("/x", &[&huge], &[Observation { value: "ok", at }], 2).err(),
        Some(sig::Error::ResourceLimit)
    );
    for value in [&long, &raw] {
        assert_eq!(
            sig::completeness("/x", &[Some(value)], &[], 5).err(),
            Some(sig::Error::ResourceLimit)
        );
    }
    let text = "x".repeat(4096);
    let repeated = vec![text.as_str(); 257];
    assert_eq!(
        sig::enumeration("/x", &repeated, &[], 1).err(),
        Some(sig::Error::ResourceLimit)
    );
    let repeated = vec![Some(&long); 4097];
    assert_eq!(
        sig::charset("/x", &repeated, &[], 11).err(),
        Some(sig::Error::ResourceLimit)
    );
    assert_eq!(
        sig::unit("/x", &vec![0.0; 4097], &[1.0], 1).err(),
        Some(sig::Error::Unit(agw_stats::unit::Error::ResourceLimit))
    );
    Ok(())
}
#[test]
fn enum_accepts_full_union_and_rejects_excess_categories_or_family_understatement()
-> Result<(), Box<dyn std::error::Error>> {
    let at = DateTime::from_timestamp(0, 0).ok_or("timestamp")?;
    let a: Vec<_> = (0..256).map(|i| format!("a{i}")).collect();
    let b: Vec<_> = (0..256).map(|i| format!("b{i}")).collect();
    let base: Vec<_> = a.iter().cycle().take(4096).map(String::as_str).collect();
    let observed: Vec<_> = b
        .iter()
        .cycle()
        .take(4096)
        .map(|value| Observation { value, at })
        .collect();
    let report = sig::enumeration("/x", &base, &observed, 512)?;
    assert_eq!(report.tests().len(), 512);
    assert!(
        report
            .tests()
            .iter()
            .all(|test| test.samples() == (4096, 4096))
    );
    assert_eq!(
        sig::enumeration("/x", &base, &observed, 511).err(),
        Some(sig::Error::InvalidInput)
    );
    let excess: Vec<_> = (0..257).map(|i| format!("a{i}")).collect();
    let excess: Vec<_> = excess.iter().map(String::as_str).collect();
    assert_eq!(
        sig::enumeration("/x", &excess, &observed, 1024).err(),
        Some(sig::Error::ResourceLimit)
    );
    let observed: Vec<_> = excess
        .iter()
        .map(|value| Observation { value, at })
        .collect();
    assert_eq!(
        sig::enumeration("/x", &base, &observed, 1024).err(),
        Some(sig::Error::ResourceLimit)
    );
    Ok(())
}
#[test]
fn global_family_total_is_retained_and_bonferroni_runs_only_in_ti19()
-> Result<(), Box<dyn std::error::Error>> {
    let yes = Value::Bool(true);
    let a = [Some(&yes); 20];
    let b: Vec<_> = (0..20)
        .map(|i| if i < 5 { None } else { Some(&yes) })
        .collect();
    let small = sig::completeness("/x", &a, &b, 5)?;
    let global = sig::completeness("/x", &a, &b, 4096)?;
    let floors = control::Floors::new(0.5, 0.2, 0.4, &[])?;
    assert_eq!(
        small.tests()[0].evidence().p_value,
        global.tests()[0].evidence().p_value
    );
    assert_eq!(global.tests()[0].evidence().tests, 4096);
    assert!(matches!(
        small.decision(0, &floors, "t")?,
        Some(control::Decision::Eligible { .. })
    ));
    assert_eq!(
        global.decision(0, &floors, "t")?,
        Some(control::Decision::NotSignificant)
    );
    assert_eq!(
        global.decision(100, &floors, "t"),
        Err(control::Error::InvalidInput)
    );
    Ok(())
}
#[test]
fn stable_fields_preserve_all_features_and_zero_effects() -> Result<(), Box<dyn std::error::Error>>
{
    let yes = Value::Bool(true);
    let samples = [Some(&yes); 20];
    let comp = sig::completeness("/x", &samples, &samples, 5)?;
    let ty = sig::coercion("/x", &samples, &samples, 6)?;
    for report in [&comp, &ty] {
        assert!(report.sufficient());
        assert!(
            report
                .tests()
                .iter()
                .all(|test| test.evidence().p_value == 1.0
                    && test.evidence().effect == 0.0
                    && !test.is_finding())
        );
    }
    let null = Value::Null;
    let unavailable = sig::coercion("/x", &[Some(&null); 100], &[None; 100], 6)?;
    assert!(!unavailable.sufficient());
    assert!(unavailable.tests().is_empty());
    Ok(())
}
