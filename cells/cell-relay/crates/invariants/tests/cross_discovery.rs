use agw_invariants::cross_endpoint::{
    Endpoint, Side,
    discovery::{Policy, Profile, Sample, discover},
};
fn endpoint(operation: &str) -> Endpoint {
    Endpoint {
        operation: operation.into(),
        side: Side::Response,
        field: "/amount".into(),
    }
}
fn samples(values: &[i64]) -> Vec<Sample> {
    values
        .iter()
        .enumerate()
        .map(|(i, v)| Sample {
            correlation: format!("entity-{i}"),
            value: Some(*v),
            at_ms: i as i64 * 1000,
        })
        .collect()
}
fn profile(operation: &str, values: &[i64]) -> Profile {
    Profile {
        endpoint: endpoint(operation),
        concept: "gross".into(),
        unit: "cent".into(),
        complete: true,
        samples: samples(values),
    }
}
fn policy() -> Policy {
    Policy {
        minimum_pairs: 3,
        tolerance: 1,
        max_age_ms: 300_000,
        max_skew_ms: 120_000,
    }
}
#[test]
fn varied_paired_profiles_discover_candidates_without_activation()
-> Result<(), Box<dyn std::error::Error>> {
    let candidates = discover(
        &[
            profile("charge", &[100, 200, 300]),
            profile("balance", &[101, 201, 301]),
        ],
        policy(),
    )?;
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].concept, "gross");
    assert_eq!(candidates[0].tolerance, 1);
    assert_eq!(candidates[0].left, endpoint("charge"));
    Ok(())
}
#[test]
fn marginals_constants_partial_and_incompatible_evidence_do_not_prove_equivalence()
-> Result<(), Box<dyn std::error::Error>> {
    for profiles in [
        vec![
            profile("a", &[100, 200, 300]),
            profile("b", &[300, 100, 200]),
        ],
        vec![
            profile("a", &[100, 100, 100]),
            profile("b", &[100, 100, 100]),
        ],
        vec![profile("a", &[100, 200]), profile("b", &[100, 200])],
        vec![
            profile("a", &[100, 200, 300]),
            Profile {
                complete: false,
                ..profile("b", &[100, 200, 300])
            },
        ],
        vec![
            profile("a", &[100, 200, 300]),
            Profile {
                unit: "dollar".into(),
                ..profile("b", &[100, 200, 300])
            },
        ],
    ] {
        assert!(discover(&profiles, policy())?.is_empty());
    }
    let a = profile("a", &[100, 200, 300]);
    let mut b = profile("b", &[100, 200, 300]);
    b.samples[0].correlation = "unrelated".into();
    assert!(discover(&[a.clone(), b], policy())?.is_empty());
    let mut b = profile("b", &[100, 200, 300]);
    b.samples[0].value = None;
    assert!(discover(&[a, b], policy())?.is_empty());
    Ok(())
}
#[test]
fn malformed_and_oversized_profile_batches_are_rejected() {
    let mut p = profile("a", &[100, 200, 300]);
    p.samples[1].correlation = p.samples[0].correlation.clone();
    assert!(discover(&[p], policy()).is_err());
    assert!(discover(&vec![profile("a", &[100, 200, 300]); 17], policy()).is_err());
    assert!(
        discover(
            &[],
            Policy {
                minimum_pairs: 0,
                ..policy()
            }
        )
        .is_err()
    );
}
