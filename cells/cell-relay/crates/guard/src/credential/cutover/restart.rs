//! Bounded restart validation: recorded probe evidence proves completion flags.
use super::{
    Cutover, Definition, Kind, Phase, Receipt, credential_failure, matches_definition, reference,
    successful_probe,
};
use std::sync::atomic::Ordering;
type Snapshot = (Receipt, Option<u64>, u64, usize, Vec<Definition>, Vec<bool>);

fn evidence_matches(
    receipt: &Receipt,
    plan: &[Definition],
    passed: &[bool],
    successes: usize,
) -> bool {
    plan.len() == passed.len()
        && receipt
            .probes
            .iter()
            .all(|p| plan.iter().any(|d| matches_definition(p, d)))
        && successes
            == receipt
                .probes
                .iter()
                .filter(|p| successful_probe(p))
                .count()
        && plan.iter().zip(passed).all(|(d, passed)| {
            *passed
                == receipt
                    .probes
                    .iter()
                    .any(|p| matches_definition(p, d) && successful_probe(p))
        })
}

impl<B> Cutover<B> {
    pub fn snapshot(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(&(
            &self.receipt,
            self.started,
            self.last_tick,
            self.successes,
            &self.plan,
            &self.passed,
        ))
    }
    pub fn restore(mut machine: Self, snapshot: Option<&str>) -> Self {
        machine.selected.store(false, Ordering::Release);
        let parsed = snapshot
            .filter(|s| s.len() <= 4 * 1024 * 1024)
            .and_then(|s| serde_json::from_str::<Snapshot>(s).ok());
        if let Some((receipt, started, last_tick, successes, plan, passed)) = parsed {
            let plan_valid = plan.len() <= machine.config.max_probes
                && passed.len() == plan.len()
                && plan.iter().all(|p| {
                    reference(&p.operation)
                        && (p.kind == Kind::Read || (p.write_opt_in && p.idempotency_key_present))
                })
                && !plan
                    .iter()
                    .enumerate()
                    .any(|(i, p)| plan[..i].iter().any(|q| q.operation == p.operation));
            let after_promotion = matches!(
                receipt.phase,
                Phase::Overlap | Phase::Promoted | Phase::Reverted | Phase::Retired
            );
            let selected = matches!(
                receipt.phase,
                Phase::Overlap | Phase::Promoted | Phase::Retired
            );
            let window_valid = if after_promotion {
                machine.config.dual_key == Some(true)
                    && receipt
                        .window_start
                        .zip(receipt.window_end)
                        .is_some_and(|(a, b)| {
                            b.checked_sub(a) == Some(machine.config.overlap_ms)
                                && started.is_some_and(|start| {
                                    a >= start && a - start < machine.config.deadline
                                })
                                && (receipt.phase != Phase::Retired || last_tick >= b)
                                && (!matches!(receipt.phase, Phase::Overlap | Phase::Promoted)
                                    || last_tick < b)
                        })
                    && !passed.is_empty()
                    && passed.iter().all(|passed| *passed)
                    && successes > 0
                    && receipt.failure_count <= machine.config.threshold
                    && (!selected
                        || (receipt.failure_count < machine.config.threshold
                            && receipt.failing_operation.is_none()
                            && receipt.audit_ref.is_none()))
            } else {
                receipt.window_start.is_none() && receipt.window_end.is_none()
            };
            let untouched = !matches!(receipt.phase, Phase::Idle | Phase::Unsupported)
                || (started.is_none()
                    && receipt.probes.is_empty()
                    && successes == 0
                    && passed.iter().all(|passed| !passed));
            let canary_valid = receipt.phase != Phase::Canary
                || (!plan.is_empty()
                    && started.is_some_and(|start| {
                        last_tick.saturating_sub(start) < machine.config.deadline
                    })
                    && (receipt.probes.len() < machine.config.max_probes
                        || passed.iter().all(|passed| *passed)));
            let valid = receipt.id == machine.receipt.id
                && (receipt.phase == Phase::Unsupported) == receipt.unsupported_reason.is_some()
                && receipt.canary
                && plan_valid
                && window_valid
                && untouched
                && canary_valid
                && receipt.probes.len() <= machine.config.max_probes
                && evidence_matches(&receipt, &plan, &passed, successes)
                && (!after_promotion && receipt.phase != Phase::Canary
                    || !receipt.probes.iter().any(credential_failure))
                && receipt.failing_operation.as_deref().is_none_or(reference)
                && receipt.audit_ref.as_deref().is_none_or(reference)
                && receipt.probes.iter().all(|p| {
                    reference(&p.operation)
                        && reference(&p.audit_ref)
                        && (100..=599).contains(&p.status)
                });
            if valid {
                machine.selected.store(selected, Ordering::Release);
                machine.receipt = receipt;
                machine.started = started;
                machine.last_tick = last_tick;
                machine.successes = successes;
                machine.plan = plan;
                machine.passed = passed;
                return machine;
            }
        }
        machine.receipt.phase = Phase::Unknown;
        machine
    }
}
