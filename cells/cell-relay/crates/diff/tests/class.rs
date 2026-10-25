//! TI-07 — the wire vocabulary of a drift classification.
//!
//! Contract §3 lists these as `X-Agindow-Drift-Class` values, and customers match
//! on them. They are pinned here because renaming one is a breaking change to
//! every integration, not a refactor.

use agw_diff::DriftClass;

#[test]
fn names_the_three_contract_wire_values() {
    assert_eq!(DriftClass::Shape.as_str(), "shape");
    assert_eq!(DriftClass::Semantic.as_str(), "semantic");
    assert_eq!(DriftClass::Unknown.as_str(), "unknown");
}

#[test]
fn round_trips_every_class_through_its_wire_value() {
    for class in [DriftClass::Shape, DriftClass::Semantic, DriftClass::Unknown] {
        assert_eq!(DriftClass::parse(class.as_str()), Some(class));
    }
}

#[test]
fn refuses_a_value_the_contract_does_not_define() {
    assert_eq!(DriftClass::parse("structural"), None);
    assert_eq!(DriftClass::parse(""), None);
}

/// Uncertainty resolves to `unknown`, never to a guess (EPIC-03 exit criteria).
#[test]
fn unknown_is_the_wire_value_for_uncertainty() {
    assert_eq!(DriftClass::Unknown.as_str(), "unknown");
}
