use super::*;
use crate::{MAX_RULES, valid_text};

pub(super) fn scope_valid(s: &Scope) -> bool {
    s.contract > 0
        && [&s.tenant, &s.connection, &s.vendor, &s.operation]
            .iter()
            .all(|v| valid_text(v))
}
pub(super) fn policy_valid(p: &Policy) -> bool {
    let t = p.thresholds;
    p.revision > 0
        && p.expires > 0
        && p.versions.0 > 0
        && p.versions.0 <= p.versions.1
        && !p.stages.is_empty()
        && p.stages.len() <= 3
        && !p.modes.is_empty()
        && p.modes.len() <= 3
        && p.families.len() <= 6
        && p.attestations.len() <= MAX_RULES
        && p.peer_attestations.len() <= crate::cross_endpoint::MAX_RULES
        && p.peer_attestations
            .iter()
            .all(|a| valid_text(&a.revision_semantics) && a.rule.validate().is_ok())
        && t.minimum_valid > 0
        && t.minimum_invalid > 0
        && t.coverage_ppm > 0
        && t.coverage_ppm <= 1_000_000
        && t.false_positive_ppm <= 1_000_000
        && t.missed_violation_ppm <= 1_000_000
        && p.attestations
            .iter()
            .all(|a| valid_text(&a.semantics) && definition_valid(&a.rule))
        && p.calibration.as_ref().is_none_or(|c| {
            valid_text(&c.dataset)
                && valid_text(&c.sampling)
                && c.candidate > 0
                && counts_valid(c.counts)
        })
}
pub(super) fn counts_valid(c: Counts) -> bool {
    c.valid.checked_add(c.invalid) == Some(c.evaluated)
        && c.evaluated <= c.eligible
        && c.false_positives <= c.valid
        && c.missed_violations <= c.invalid
}
pub(super) fn gate(c: Counts, t: Thresholds) -> Reason {
    if c.valid < t.minimum_valid
        || c.invalid < t.minimum_invalid
        || c.eligible == 0
        || u128::from(c.evaluated) * 1_000_000 < u128::from(c.eligible) * u128::from(t.coverage_ppm)
    {
        return Reason::Insufficient;
    }
    if rates_fail(c, t) {
        Reason::Negative
    } else {
        Reason::Qualified
    }
}
fn rates_fail(c: Counts, t: Thresholds) -> bool {
    u128::from(c.false_positives) * 1_000_000
        > u128::from(c.valid) * u128::from(t.false_positive_ppm)
        || u128::from(c.missed_violations) * 1_000_000
            > u128::from(c.invalid) * u128::from(t.missed_violation_ppm)
}

pub(super) fn sum(a: Counts, b: Counts) -> Result<Counts, Error> {
    let add = |x: u64, y: u64| x.checked_add(y).ok_or(Error::Capacity);
    Ok(Counts {
        eligible: add(a.eligible, b.eligible)?,
        evaluated: add(a.evaluated, b.evaluated)?,
        valid: add(a.valid, b.valid)?,
        invalid: add(a.invalid, b.invalid)?,
        false_positives: add(a.false_positives, b.false_positives)?,
        missed_violations: add(a.missed_violations, b.missed_violations)?,
    })
}
pub(super) fn qualify(
    c: &Controller,
    candidate: &Candidate,
    stage: Stage,
    now: u64,
    receipts: &[Receipt],
    counts: Counts,
) -> Reason {
    let Some(p) = &c.policy else {
        return Reason::Unauthorized;
    };
    if let Some((reason, _)) = &c.fault {
        return *reason;
    }
    if now >= p.expires {
        return Reason::Expired;
    }
    if c.controls.pin.is_some_and(|v| v != candidate.version)
        || candidate.rules.iter().any(|r| {
            c.controls.disabled.contains(&r.id) || c.controls.overrides.iter().any(|o| o.id == r.id)
        })
    {
        return Reason::CustomerControl;
    }
    if candidate.version < p.versions.0 || candidate.version > p.versions.1 {
        return Reason::Unauthorized;
    }
    if !p.stages.contains(&stage) {
        return Reason::Unauthorized;
    }
    if candidate
        .rules
        .iter()
        .any(|r| !p.families.contains(&Family::of(r)))
    {
        return Reason::Family;
    }
    if candidate.rules.iter().any(|r| {
        Family::of(r) != Family::Enumeration && !p.attestations.iter().any(|a| a.rule == *r)
    }) {
        return Reason::Attestation;
    }
    if !candidate.peers.is_empty() && !p.families.contains(&Family::CrossEndpoint) {
        return Reason::Family;
    }
    if candidate
        .peers
        .iter()
        .any(|r| !p.peer_attestations.iter().any(|a| a.rule == *r))
    {
        return Reason::Attestation;
    }
    if candidate
        .peers
        .iter()
        .any(|r| c.controls.disabled.contains(&r.id))
    {
        return Reason::CustomerControl;
    }
    if c.active.as_ref().is_some_and(|g| {
        g.stage() != Stage::Shadow
            && (g.candidate != *candidate
                || g.decision
                    .authorization
                    .as_ref()
                    .is_some_and(|old| old.modes.iter().any(|m| !p.modes.contains(m))))
    }) {
        return Reason::CustomerControl;
    }
    let previous = c.active.as_ref().filter(|g| g.candidate == *candidate);
    let expected = previous.map_or(Stage::Shadow, |g| match g.stage() {
        Stage::Shadow => Stage::Reads,
        _ => Stage::Blocking,
    });
    if stage != expected || previous.is_some_and(|g| g.stage() == Stage::Blocking) {
        return Reason::StageOrder;
    }
    if c.active
        .as_ref()
        .is_some_and(|g| candidate.version < g.candidate.version)
    {
        return Reason::CustomerControl;
    }
    // Revision changes do not erase adverse or incomplete historical windows.
    let history: Vec<_> = c
        .receipts
        .iter()
        .filter(|r| c.related(r.candidate, candidate))
        .collect();
    for (i, r) in history.iter().enumerate() {
        if !r.independent
            || r.training == r.holdout
            || !counts_valid(r.counts)
            || r.started > r.finished
            || r.finished > now
            || r.finished >= r.expires
            || history[..i].iter().any(|other| {
                other.holdout == r.holdout
                    || other.holdout == r.training
                    || other.training == r.holdout
            })
        {
            return Reason::Contradictory;
        }
        if !r.complete {
            return Reason::Incomplete;
        }
        if now >= r.expires {
            return Reason::Expired;
        }
        if rates_fail(r.counts, p.thresholds) {
            return Reason::Negative;
        }
    }
    if receipts.is_empty() {
        return Reason::Insufficient;
    }
    for (i, r) in receipts.iter().enumerate() {
        if !r.independent
            || r.training == r.holdout
            || !counts_valid(r.counts)
            || r.started > r.finished
            || r.finished > now
            || r.finished >= r.expires
            || receipts[..i].iter().any(|other| {
                other.holdout == r.holdout
                    || other.training == r.holdout
                    || other.holdout == r.training
            })
            || c.receipts.iter().any(|other| {
                other.candidate == r.candidate
                    && other.stage != r.stage
                    && (other.holdout == r.holdout || other.holdout == r.training)
            })
        {
            return Reason::Contradictory;
        }
        if !r.complete {
            return Reason::Incomplete;
        }
        if now >= r.expires {
            return Reason::Expired;
        }
        if previous.is_some_and(|g| r.started <= g.decision.at) {
            return Reason::Contradictory;
        }
        // A bad independent window cannot be diluted with a larger good one.
        if rates_fail(r.counts, p.thresholds) {
            return Reason::Negative;
        }
    }
    if stage == Stage::Blocking
        && !p.calibration.as_ref().is_some_and(|cal| {
            cal.candidate == candidate.version
                && cal.independent_production
                && gate(cal.counts, p.thresholds) == Reason::Qualified
        })
    {
        return Reason::Calibration;
    }
    gate(counts, p.thresholds)
}

// Validate administrative definitions before retaining or cloning authorization.
pub(super) fn definition_valid(rule: &crate::Rule) -> bool {
    use crate::Form;
    let mut bindings: Vec<&str> = match &rule.form {
        Form::Range { field, .. } | Form::Enum { field, .. } => vec![field],
        Form::Relation { left, right, .. } | Form::CrossEndpoint { left, right, .. } => {
            vec![left, right]
        }
        Form::Ordering { earlier, later } => vec![earlier, later],
        Form::Sum { total, terms, .. } => {
            if terms.len() > crate::MAX_TERMS {
                return false;
            }
            std::iter::once(total.as_str())
                .chain(terms.iter().map(String::as_str))
                .collect()
        }
    };
    bindings.sort_unstable();
    bindings.dedup();
    crate::compile(std::slice::from_ref(rule), &bindings).is_ok()
}
pub(super) fn same_definitions(a: &Candidate, b: &Candidate) -> bool {
    a.rules.len() == b.rules.len()
        && a.rules.iter().all(|r| b.rules.contains(r))
        && a.bindings.len() == b.bindings.len()
        && a.bindings.iter().all(|v| b.bindings.contains(v))
        && a.peers.len() == b.peers.len()
        && a.peers.iter().all(|v| b.peers.contains(v))
}
