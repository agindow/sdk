use agw_inspector::Value;
use agw_stats::{
    control::{Decision, Floors},
    enumeration, field, significance as sig, unit,
};
use chrono::DateTime;

#[test]
fn numeric_adapter_preserves_diagnosis_and_drives_real_ti19_eligibility()
-> Result<(), Box<dyn std::error::Error>> {
    let a = [1.0; 100];
    let b = [100.0; 100];
    let report = sig::unit("/amount", &a, &b, 1)?;
    assert_eq!(report.detection(), &unit::detect("/amount", &a, &b)?);
    assert!(report.sufficient());
    assert_eq!(report.tests().len(), 1);
    let test = &report.tests()[0];
    assert_eq!(test.hypothesis(), &sig::Hypothesis::Magnitude);
    assert_eq!(test.samples(), (100, 100));
    assert_eq!(test.evidence().effect, 1.0);
    assert!(test.evidence().p_value < 0.001);
    let floors = Floors::new(0.05, 0.2, 0.4, &[])?;
    assert!(matches!(
        report.decision(0, &floors, "t")?,
        Some(Decision::Eligible { .. })
    ));
    let stable = sig::unit("/amount", &a, &a, 1)?;
    assert_eq!(stable.tests()[0].evidence().p_value, 1.0);
    assert_eq!(stable.decision(0, &floors, "t")?, None);
    Ok(())
}
#[test]
fn enum_uses_occurrence_counts_and_tests_known_categories_before_filtering()
-> Result<(), Box<dyn std::error::Error>> {
    let at = DateTime::from_timestamp(0, 0).ok_or("timestamp")?;
    let a = vec!["ok"; 100];
    let b: Vec<_> = (0..100)
        .map(|i| enumeration::Observation {
            value: if i < 50 { "new" } else { "ok" },
            at,
        })
        .collect();
    let report = sig::enumeration("/status", &a, &b, 2)?;
    assert_eq!(
        report.detection(),
        &enumeration::compare("/status", &["ok"], &b)?
    );
    assert_eq!(report.tests().len(), 2);
    let floors = Floors::new(0.05, 0.1, 0.4, &[])?;
    for (index, test) in report.tests().iter().enumerate() {
        assert_eq!(test.samples(), (100, 100));
        assert_eq!(test.evidence().tests, 2);
        assert_eq!(test.evidence().effect, 0.5);
        assert!(test.evidence().p_value < 0.001);
        if test.hypothesis() == &sig::Hypothesis::Category("new".into()) {
            assert!(test.is_finding());
            assert!(matches!(
                report.decision(index, &floors, "t")?,
                Some(Decision::Eligible { .. })
            ));
        } else {
            assert!(!test.is_finding());
            assert_eq!(report.decision(index, &floors, "t")?, None);
        }
    }
    assert!(sig::enumeration("/status", &a, &b, 1).is_err());
    // A valid but weak novelty finding must not be stamped p=0 or promoted.
    let b: Vec<_> = (0..20)
        .map(|i| enumeration::Observation {
            value: if i < 2 { "new" } else { "ok" },
            at,
        })
        .collect();
    let weak = sig::enumeration("/status", &["ok"; 20], &b, 2)?;
    let index = weak
        .tests()
        .iter()
        .position(|t| t.is_finding())
        .ok_or("novelty finding")?;
    assert_eq!(
        weak.decision(index, &floors, "t")?,
        Some(Decision::NotSignificant)
    );
    Ok(())
}
#[test]
fn completeness_tests_all_five_features_with_opportunity_denominators()
-> Result<(), Box<dyn std::error::Error>> {
    let yes = Value::Bool(true);
    let a = [Some(&yes); 100];
    let b = [None; 100];
    let report = sig::completeness("/x", &a, &b, 5)?;
    assert_eq!(report.detection(), &field::completeness("/x", &a, &b)?);
    assert_eq!(report.tests().len(), 5);
    let floors = Floors::new(0.05, 0.1, 0.4, &[])?;
    let mut findings = 0;
    for (index, test) in report.tests().iter().enumerate() {
        assert_eq!(test.samples(), (100, 100));
        assert_eq!(test.evidence().tests, 5);
        if test.is_finding() {
            findings += 1;
            assert_eq!(test.evidence().effect, 1.0);
            assert!(matches!(
                report.decision(index, &floors, "t")?,
                Some(Decision::Eligible { .. })
            ));
        } else {
            assert_eq!(test.evidence().p_value, 1.0);
            assert_eq!(report.decision(index, &floors, "t")?, None);
        }
    }
    assert_eq!(findings, 2);
    Ok(())
}
#[test]
fn type_denominator_excludes_null_and_missing_and_retains_directional_gate()
-> Result<(), Box<dyn std::error::Error>> {
    let text = Value::String("yes".into());
    let yes = Value::Bool(true);
    let null = Value::Null;
    let a: Vec<_> = (0..100)
        .map(|i| if i < 50 { Some(&text) } else { Some(&null) })
        .collect();
    let b: Vec<_> = (0..100)
        .map(|i| if i < 50 { Some(&yes) } else { None })
        .collect();
    let report = sig::coercion("/x", &a, &b, 6)?;
    assert_eq!(report.detection(), &field::coercion("/x", &a, &b)?);
    assert_eq!(report.tests().len(), 6);
    for test in report.tests() {
        assert_eq!(test.samples(), (50, 50));
        assert_eq!(
            test.is_finding(),
            test.hypothesis() == &sig::Hypothesis::Type(field::Type::String)
        );
    }
    Ok(())
}
#[test]
fn charset_tests_all_classes_and_formats_using_string_counts()
-> Result<(), Box<dyn std::error::Error>> {
    let date = Value::String("2026-10-17".into());
    let month = Value::String("Oct 17".into());
    let null = Value::Null;
    let a: Vec<_> = (0..100)
        .map(|i| if i < 50 { Some(&date) } else { None })
        .collect();
    let b: Vec<_> = (0..100)
        .map(|i| if i < 50 { Some(&month) } else { Some(&null) })
        .collect();
    let report = sig::charset("/date", &a, &b, 11)?;
    assert_eq!(report.detection(), &field::charset("/date", &a, &b)?);
    assert_eq!(report.tests().len(), 11);
    let emerging = report
        .tests()
        .iter()
        .find(|test| test.hypothesis() == &sig::Hypothesis::Format(field::Format::MonthDay))
        .ok_or("month format")?;
    assert!(emerging.is_finding());
    assert!(emerging.evidence().p_value < 0.001);
    for test in report.tests() {
        assert_eq!(test.samples(), (50, 50));
    }
    Ok(())
}
#[test]
fn insufficient_windows_carry_no_invented_p_values() -> Result<(), Box<dyn std::error::Error>> {
    let yes = Value::Bool(true);
    let unit = sig::unit("/x", &[1.0; 19], &[100.0; 100], 1)?;
    assert!(!unit.sufficient());
    assert!(unit.tests().is_empty());
    let at = DateTime::from_timestamp(0, 0).ok_or("timestamp")?;
    let observed = [enumeration::Observation { value: "new", at }; 100];
    let en = sig::enumeration("/x", &["old"; 19], &observed, 2)?;
    assert!(!en.sufficient());
    assert!(en.tests().is_empty());
    let field = sig::completeness("/x", &[Some(&yes); 19], &[None; 100], 5)?;
    assert!(!field.sufficient());
    assert!(field.tests().is_empty());
    let strings = sig::charset("/x", &[Some(&yes); 100], &[Some(&yes); 100], 11)?;
    assert!(!strings.sufficient());
    assert!(strings.tests().is_empty());
    Ok(())
}
#[test]
fn rejects_underdeclared_families_before_detector_execution() {
    let invalid = Value::Number(agw_inspector::Number::Verbatim("bad".into()));
    let samples = [Some(&invalid); 20];
    for total in [0, 4, 4097] {
        assert_eq!(
            sig::completeness("/x", &samples, &samples, total).err(),
            Some(sig::Error::InvalidInput)
        );
    }
    assert_eq!(
        sig::coercion("/x", &samples, &samples, 5).err(),
        Some(sig::Error::InvalidInput)
    );
    assert_eq!(
        sig::charset("/x", &samples, &samples, 10).err(),
        Some(sig::Error::InvalidInput)
    );
}
