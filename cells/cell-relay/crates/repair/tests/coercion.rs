//! TI-07 — the repair path refuses numeric coercion.
//!
//! Contract §3.1 requirement 3: *"Never coerce numerics. If a field must change
//! type, string→number on an ID is a corruption, not a repair. Reject and classify
//! as `semantic`."*
//!
//! The distinction this suite protects: a repair that changes a *value* is the
//! product. A repair that changes a value's *type* is how `charge.id` stops being
//! the charge's ID. TI-12 builds the engine that proposes repairs; this rule is the
//! one proposal it must never be allowed to make, and these cases are the fence.

use agw_diff::DriftClass;
use agw_repair::coercion::{JsonType, Proposal, Refusal, check};

/// Ok when the proposal is not a numeric coercion.
fn verdict(path: &str, from: JsonType, to: JsonType) -> Result<(), Refusal<'_>> {
    check(&Proposal { path, from, to })
}

fn refuses(from: JsonType, to: JsonType) -> bool {
    verdict("charge.id", from, to).is_err()
}

/// The refusal for a proposal that must be refused. The workspace denies
/// `expect_used` in tests too, so this is a match — which also reports both types
/// on failure, where `expect_err` would only report the path.
fn refusal(path: &str, from: JsonType, to: JsonType) -> Refusal<'_> {
    match verdict(path, from, to) {
        Err(refusal) => refusal,
        Ok(()) => panic!("{path}: {from} -> {to} was admitted, but is a coercion"),
    }
}

// ------------------------------------------------------------------ refused

/// The canonical corruption: an ID that arrived as a string, "repaired" into a
/// number. Whatever the repair believed it was doing, the identity changed type.
#[test]
fn refuses_string_to_number() {
    assert!(refuses(JsonType::String, JsonType::Number));
}

/// The mirror: a number rendered as a string. Downstream that string stops being
/// comparable to any other number and starts being a lookup key.
#[test]
fn refuses_number_to_string() {
    assert!(refuses(JsonType::Number, JsonType::String));
}

/// Any type change involving a number is a coercion. Numbers are unforgiving in
/// both directions because neither side has enough information to reconstruct the
/// other faithfully.
#[test]
fn refuses_every_type_change_touching_a_number() {
    let others = [
        JsonType::Null,
        JsonType::Bool,
        JsonType::Array,
        JsonType::Object,
    ];
    for other in others {
        assert!(refuses(JsonType::Number, other), "number -> {other:?}");
        assert!(refuses(other, JsonType::Number), "{other:?} -> number");
    }
}

/// A refused coercion is `semantic`, always. The caller never gets to choose, and
/// never has to remember: contract §3.1 names the classification.
#[test]
fn a_refused_coercion_is_classified_semantic() {
    let refusal = refusal("charge.id", JsonType::String, JsonType::Number);
    assert_eq!(refusal.class, DriftClass::Semantic);
}

/// The refusal names the path and both types, so the audit artifact and the loud
/// path (§4.3) can explain it without re-deriving anything.
#[test]
fn a_refusal_names_the_path_and_both_types() {
    let refusal = refusal("charge.id", JsonType::String, JsonType::Number);
    assert_eq!(refusal.path, "charge.id");
    assert_eq!(refusal.from, JsonType::String);
    assert_eq!(refusal.to, JsonType::Number);
    // "Never silent" (contract §4.3): the message has to survive to a human.
    let message = refusal.to_string();
    assert!(
        message.contains("charge.id"),
        "refusal omits the path: {message}"
    );
    assert!(
        message.contains("string"),
        "refusal omits the from-type: {message}"
    );
    assert!(
        message.contains("number"),
        "refusal omits the to-type: {message}"
    );
}

// ------------------------------------------------------------------ permitted

/// The rule is about *types*, not about numbers changing. A unit flip —
/// `9007199254740993` cents becoming dollars — is the product working, and it
/// stays permitted. Forbidding it would forbid selective repair outright.
#[test]
fn permits_a_number_to_a_different_number() {
    assert!(verdict("charge.amount", JsonType::Number, JsonType::Number).is_ok());
}

/// A rename is a string-to-string change and is the dominant write-side repair.
#[test]
fn permits_a_string_to_a_different_string() {
    assert!(verdict("charge.source", JsonType::String, JsonType::String).is_ok());
}

/// Everything else is untouched by this rule; other invariants own it.
#[test]
fn permits_type_changes_that_involve_no_number() {
    for (from, to) in [
        (JsonType::String, JsonType::Bool),
        (JsonType::Bool, JsonType::String),
        (JsonType::Null, JsonType::String),
        (JsonType::Array, JsonType::Object),
        (JsonType::Object, JsonType::Array),
    ] {
        assert!(verdict("any.field", from, to).is_ok(), "{from:?} -> {to:?}");
    }
}

/// A proposal that changes nothing cannot corrupt anything.
#[test]
fn permits_a_no_op_even_for_a_number() {
    assert!(verdict("charge.id", JsonType::Number, JsonType::Number).is_ok());
}
