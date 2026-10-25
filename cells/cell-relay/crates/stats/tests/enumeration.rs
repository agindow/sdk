use agw_diff::{DriftClass, Side};
use agw_stats::enumeration::{Error, Evidence, Observation, compare};
use chrono::{TimeZone, Utc};

fn observation(value: &str, seconds: i64) -> Observation<'_> {
    Observation {
        value,
        at: Utc
            .timestamp_opt(seconds, 0)
            .single()
            .unwrap_or_else(|| panic!("test time")),
    }
}
fn window(value: &str, count: usize) -> Vec<Observation<'_>> {
    vec![observation(value, 100); count]
}

#[test]
fn additions_report_cardinalities_and_per_value_evidence() -> Result<(), Error> {
    let mut samples = window("pending", 16);
    samples.extend([
        observation("review", 9),
        observation("review", 3),
        observation("failed", 20),
        observation("failed", 30),
    ]);
    let r = compare("status", &["pending", "done"], &samples)?;
    assert_eq!(r.cardinalities().baseline, 2);
    assert_eq!(r.cardinalities().observed, 3);
    assert_eq!(r.cardinalities().union, 4);
    assert_eq!(r.evidence(), Evidence::Semantic);
    assert_eq!(r.class(), Some(DriftClass::Semantic));
    let new = r.new_values();
    assert_eq!(new.len(), 2);
    assert_eq!(new[0].value, "failed");
    assert_eq!(new[0].count, 2);
    assert_eq!(new[0].frequency, 0.1);
    assert_eq!(new[0].first_seen, observation("", 20).at);
    assert!(new[0].material);
    assert_eq!(new[1].value, "review");
    assert_eq!(new[1].first_seen, observation("", 3).at);
    assert_eq!(samples[17].at, observation("", 3).at);
    Ok(())
}

#[test]
fn replacement_is_semantic_even_when_observed_cardinality_falls() -> Result<(), Error> {
    for baseline in [vec!["old"], vec!["old", "another"]] {
        let r = compare("status", &baseline, &window("new", 20))?;
        assert_eq!(r.evidence(), Evidence::Semantic);
        assert_eq!(r.cardinalities().observed, 1);
        assert_eq!(r.cardinalities().union, baseline.len() + 1);
    }
    Ok(())
}

#[test]
fn known_value_frequency_changes_never_trigger_enum_drift() -> Result<(), Error> {
    for value in ["pending", "done"] {
        let r = compare("status", &["pending", "done"], &window(value, 100))?;
        assert_eq!(r.evidence(), Evidence::Unchanged);
        assert_eq!(r.class(), None);
        assert!(r.new_values().is_empty());
    }
    Ok(())
}

#[test]
fn count_and_frequency_floors_are_inclusive_and_independent() -> Result<(), Error> {
    for (total, hits, expected) in [
        (20, 1, false),
        (20, 2, true),
        (200, 2, true),
        (201, 2, false),
    ] {
        let mut samples = window("known", total - hits);
        samples.extend(window("new", hits));
        let r = compare("status", &["known"], &samples)?;
        assert_eq!(r.new_values()[0].frequency, hits as f64 / total as f64);
        assert_eq!(r.new_values()[0].material, expected);
        assert_eq!(
            r.evidence(),
            if expected {
                Evidence::Semantic
            } else {
                Evidence::BelowMateriality
            }
        );
        assert_eq!(r.class(), expected.then_some(DriftClass::Semantic));
    }
    Ok(())
}

#[test]
fn volume_and_missing_baseline_are_insufficient_evidence() -> Result<(), Error> {
    for total in [0, 1, 19] {
        let r = compare("status", &["known"], &window("new", total))?;
        assert_eq!(r.evidence(), Evidence::InsufficientEvidence);
        assert_eq!(r.class(), None);
        assert!(r.new_values().iter().all(|v| !v.material));
    }
    let r = compare("status", &[], &window("new", 20))?;
    assert_eq!(r.evidence(), Evidence::InsufficientEvidence);
    assert!(r.new_values().is_empty());
    assert_eq!(r.cardinalities().baseline, 0);
    assert_eq!(r.cardinalities().observed, 1);
    assert_eq!(r.cardinalities().union, 1);
    Ok(())
}

#[test]
fn exact_values_and_fields_remain_independent() -> Result<(), Error> {
    let samples = ["", "OK", " ok", "é", "e\u{301}", "x\"\n"]
        .into_iter()
        .flat_map(|v| window(v, 4))
        .collect::<Vec<_>>();
    let r = compare("status", &["ok", "é"], &samples)?;
    assert_eq!(r.new_values().len(), 5);
    assert_eq!(r.cardinalities().union, 7);
    assert_eq!(
        compare("other", &["x"], &window("x", 20))?.evidence(),
        Evidence::Unchanged
    );
    let r = compare("status", &["ok", "é"], &samples)?;
    assert_eq!(r.new_values()[0].value, "");
    assert!(r.new_values().iter().all(|v| v.value != "é"));
    Ok(())
}

#[test]
fn resource_and_invalid_inputs_fail_before_any_evidence() {
    assert_eq!(compare("", &[], &[]).err(), Some(Error::InvalidInput));
    assert_eq!(
        compare("s", &["x", "x"], &[]).err(),
        Some(Error::InvalidInput)
    );
    assert_eq!(
        compare(&"s".repeat(4097), &[], &[]).err(),
        Some(Error::ResourceLimit)
    );
    assert_eq!(
        compare("s", &[], &window("x", 4097)).err(),
        Some(Error::ResourceLimit)
    );
    let long = "x".repeat(4097);
    assert_eq!(
        compare("s", &[&long], &[]).err(),
        Some(Error::ResourceLimit)
    );
    assert_eq!(
        compare("s", &[], &window(&long, 1)).err(),
        Some(Error::ResourceLimit)
    );
    let keys = (0..257).map(|n| n.to_string()).collect::<Vec<_>>();
    let baseline = keys.iter().map(String::as_str).collect::<Vec<_>>();
    assert_eq!(
        compare("s", &baseline, &[]).err(),
        Some(Error::ResourceLimit)
    );
    let samples = keys.iter().map(|k| observation(k, 0)).collect::<Vec<_>>();
    assert_eq!(
        compare("s", &["x"], &samples).err(),
        Some(Error::ResourceLimit)
    );
    let value = "x".repeat(4096);
    assert_eq!(
        compare("s", &["x"], &window(&value, 256)).err(),
        Some(Error::ResourceLimit)
    );
}

#[test]
fn maximum_sizes_remain_valid() -> Result<(), Error> {
    let keys = (0..256).map(|n| n.to_string()).collect::<Vec<_>>();
    let baseline = keys.iter().map(String::as_str).collect::<Vec<_>>();
    let samples = keys.iter().flat_map(|k| window(k, 16)).collect::<Vec<_>>();
    let r = compare(&"s".repeat(4096), &baseline, &samples)?;
    assert_eq!(r.evidence(), Evidence::Unchanged);
    assert_eq!(r.cardinalities().observed, 256);
    // Exactly 1 MiB of aggregate text remains valid, including the path.
    let value = "x".repeat(4096);
    assert_eq!(
        compare(&value, &[], &window(&value, 255))?.evidence(),
        Evidence::InsufficientEvidence
    );
    Ok(())
}

#[test]
fn audit_enumerates_novelty_and_clears_stale_claims() -> Result<(), Box<dyn std::error::Error>> {
    use agw_audit::contracts::audit_artifact::AuditArtifact;
    let fixture = include_str!("../../../../../contracts/examples/audit_artifact.valid.json");
    let mut v: serde_json::Value = serde_json::from_str(fixture)?;
    v["decision"] = "blocked".into();
    for field in ["hypothesis", "detail", "note"] {
        v[field] = "stale".into();
    }
    v["confidence"] = 1.0.into();
    v["metadata"]["agindow_enum_growth"] = "stale".into();
    let draft: AuditArtifact = serde_json::from_value(v)?;
    let mut samples = window("known", 17);
    samples.extend([
        observation("", 4),
        observation("", 2),
        observation("rare", 1),
    ]);
    let r = compare("status", &["known"], &samples)?;
    for (side, wire) in [(Side::Request, "request"), (Side::Response, "response")] {
        let json = serde_json::to_value(r.audit(draft.clone(), side)?)?;
        assert_eq!(json["drift"]["class"], "semantic");
        assert_eq!(json["drift"]["side"], wire);
        assert_eq!(json["drift"]["changes"].as_array().map(Vec::len), Some(1));
        assert_eq!(json["drift"]["changes"][0]["path"], "status");
        assert_eq!(json["drift"]["changes"][0]["to"], "");
        assert_eq!(json["drift"]["changes"][0]["kind"], "enum_expanded");
        assert_eq!(json["drift"]["first_seen"], "1970-01-01T00:00:02Z");
        let meta = &json["metadata"]["agindow_enum_growth"];
        assert_eq!(meta["baseline_cardinality"], 1);
        assert_eq!(meta["observed_cardinality"], 3);
        assert_eq!(meta["union_cardinality"], 3);
        assert_eq!(meta["path"], "status");
        assert_eq!(meta["new_values"][0]["count"], 2);
        assert_eq!(meta["new_values"][0]["frequency"], 0.1);
        assert_eq!(meta["new_values"][0]["first_seen"], "1970-01-01T00:00:02Z");
        assert_eq!(meta["new_values"][1]["value"], "rare");
        assert_eq!(meta["new_values"][1]["material"], false);
        assert_eq!(json["metadata"]["tenant"], "acme");
        assert_eq!(json["decision"], "blocked");
        assert!(json["patch"].is_null());
        assert_eq!(json["proof"]["write_integrity"], false);
        for field in ["hypothesis", "detail", "note", "confidence"] {
            assert!(json.get(field).is_none());
        }
        let _: AuditArtifact = serde_json::from_value(json)?;
    }
    for samples in [window("known", 20), window("new", 1)] {
        assert_eq!(
            compare("status", &["known"], &samples)?
                .audit(draft.clone(), Side::Request)
                .err(),
            Some(Error::NoFinding)
        );
    }
    Ok(())
}
