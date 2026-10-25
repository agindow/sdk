use agw_diff::rename::{Decision, Error, Field, distances, match_fields};
use agw_inspector::inference::JsonType as T;
use std::collections::BTreeMap;

fn field(name: &str, counts: &[(&str, u64)], opportunities: u64) -> Field {
    Field {
        name: name.into(),
        kind: T::String,
        binning: "exact-string-v1".into(),
        counts: counts.iter().map(|(k, v)| ((*k).into(), *v)).collect(),
        opportunities,
    }
}
fn balanced(name: &str, n: u64) -> Field {
    field(name, &[("card", n), ("bank", n)], 2 * n)
}
fn score(decision: &Decision) -> f64 {
    match decision {
        Decision::Confirmed { confidence, .. } => *confidence,
        other => panic!("{other:?}"),
    }
}
#[test]
fn identical_distributions_confirm_without_name_similarity() -> Result<(), Error> {
    let result = match_fields(
        &[balanced("source", 100)],
        &[balanced("payment_method", 200)],
    )?;
    assert_eq!(score(&result[0]), 1.0);
    assert!(matches!(result[0], Decision::Confirmed { added: 0, .. }));
    Ok(())
}
#[test]
fn formulas_match_independent_two_bin_calculation() -> Result<(), Error> {
    let a = field("a", &[("x", 50), ("y", 50)], 100);
    let b = field("b", &[("x", 60), ("y", 40)], 100);
    let d = distances(&a, &b)?.ok_or(Error::InvalidInput)?;
    assert!((d.psi - 0.04054651081081645).abs() < 1e-12);
    assert!((d.js - 0.005059389928987596).abs() < 1e-12);
    assert_eq!(Some(d), distances(&b, &a)?);
    let result = match_fields(&[a], &[b])?;
    assert!((score(&result[0]) - (1.0 - d.psi)).abs() < 1e-12);
    Ok(())
}
#[test]
fn ambiguity_is_bidirectional_and_precedes_sample_penalties() -> Result<(), Error> {
    assert_eq!(
        match_fields(
            &[balanced("old", 100)],
            &[balanced("a", 100), balanced("b", 1)]
        )?,
        vec![Decision::Ambiguous]
    );
    assert_eq!(
        match_fields(
            &[balanced("a", 100), balanced("b", 1)],
            &[balanced("new", 100)]
        )?,
        vec![Decision::Ambiguous, Decision::Ambiguous]
    );
    Ok(())
}
#[test]
fn unrelated_additions_do_not_pair() -> Result<(), Error> {
    let old = balanced("old", 100);
    let a = field("a", &[("other", 200)], 200);
    let b = field("b", &[("card", 1), ("bank", 199)], 200);
    assert_eq!(match_fields(&[old], &[a, b])?, vec![Decision::NoMatch]);
    Ok(())
}
#[test]
fn low_volume_and_sparse_matches_have_separate_penalties() -> Result<(), Error> {
    let high = balanced("old", 100);
    let low = balanced("new", 1);
    let mut sparse = balanced("new", 100);
    sparse.opportunities = 2000;
    assert!((score(&match_fields(std::slice::from_ref(&high), &[low])?[0]) - 0.902).abs() < 1e-12);
    assert!((score(&match_fields(&[high], &[sparse])?[0]) - 0.91).abs() < 1e-12);
    Ok(())
}
#[test]
fn no_evidence_and_incompatible_histograms_do_not_match() -> Result<(), Error> {
    let mut null = balanced("new", 100);
    null.kind = T::Null;
    assert_eq!(
        match_fields(&[balanced("old", 100)], &[null])?,
        vec![Decision::NoMatch]
    );
    let a = balanced("old", 100);
    let mut b = balanced("new", 100);
    b.binning = "other-bounds".into();
    assert_eq!(distances(&a, &b)?, None);
    b.binning = a.binning.clone();
    b.kind = T::Integer;
    assert_eq!(distances(&a, &b)?, None);
    b.kind = T::String;
    b.counts.clear();
    b.opportunities = 0;
    assert_eq!(match_fields(&[a], &[b])?, vec![Decision::NoMatch]);
    Ok(())
}
#[test]
fn zero_support_is_finite_and_symmetric() -> Result<(), Error> {
    let a = field("a", &[("x", 100)], 100);
    let b = field("b", &[("y", 100)], 100);
    let d = distances(&a, &b)?.ok_or(Error::InvalidInput)?;
    assert!(d.psi.is_finite() && d.psi > 0.1);
    assert!((d.js - std::f64::consts::LN_2).abs() < 1e-12);
    assert_eq!(Some(d), distances(&b, &a)?);
    Ok(())
}
#[test]
fn invalid_inputs_and_limits_never_return_partial_matches() {
    let a = balanced("old", 100);
    let mut b = balanced("new", 100);
    b.opportunities = 1;
    assert_eq!(
        match_fields(std::slice::from_ref(&a), std::slice::from_ref(&b)),
        Err(Error::InvalidInput)
    );
    b.opportunities = u64::MAX;
    b.counts = BTreeMap::from([("x".into(), u64::MAX), ("y".into(), 1)]);
    assert_eq!(distances(&a, &b), Err(Error::InvalidInput));
    b = field("new", &[("x", 0)], 1);
    assert_eq!(distances(&a, &b), Err(Error::InvalidInput));
    assert_eq!(
        match_fields(&[a.clone(), a.clone()], &[]),
        Err(Error::InvalidInput)
    );
    assert_eq!(
        match_fields(std::slice::from_ref(&a), std::slice::from_ref(&a)),
        Err(Error::InvalidInput)
    );
    assert_eq!(
        match_fields(&vec![a.clone(); 65], &[]),
        Err(Error::ResourceLimit)
    );
    b = balanced("new", 100);
    b.binning = "x".repeat(1024 * 1024);
    assert_eq!(
        match_fields(std::slice::from_ref(&a), &[b]),
        Err(Error::ResourceLimit)
    );
    b = balanced("new", 100);
    b.counts = (0..257).map(|i| (i.to_string(), 1)).collect();
    b.opportunities = 257;
    assert_eq!(distances(&a, &b), Err(Error::ResourceLimit));
}

#[test]
fn unique_pair_survives_unrelated_candidates_and_preserves_order() -> Result<(), Error> {
    let removed = [
        balanced("source", 100),
        field("old_other", &[("z", 100)], 100),
    ];
    let added = [
        field("unrelated", &[("q", 100)], 100),
        balanced("payment_method", 100),
    ];
    let decisions = match_fields(&removed, &added)?;
    assert!(matches!(
        decisions[0],
        Decision::Confirmed {
            added: 1,
            confidence: 1.0,
            ..
        }
    ));
    assert_eq!(decisions[1], Decision::NoMatch);
    assert_eq!(removed[0].name, "source");
    assert_eq!(removed[0].counts, balanced("source", 100).counts);
    assert_eq!(match_fields(&[], &added)?, Vec::new());
    assert_eq!(match_fields(&removed, &[])?, vec![Decision::NoMatch; 2]);
    Ok(())
}

#[test]
fn same_numeric_bins_preserve_large_exact_labels() -> Result<(), Error> {
    let mut a = field("old", &[("9007199254740993", 100), ("1e400", 100)], 200);
    a.kind = T::Number;
    a.binning = "raw-lexemes-v1".into();
    let mut b = a.clone();
    b.name = "new".into();
    assert_eq!(
        score(&match_fields(std::slice::from_ref(&a), std::slice::from_ref(&b))?[0]),
        1.0
    );
    b.counts = BTreeMap::from([("9007199254740992".into(), 100), ("1e399".into(), 100)]);
    assert_eq!(match_fields(&[a], &[b])?, vec![Decision::NoMatch]);
    Ok(())
}
