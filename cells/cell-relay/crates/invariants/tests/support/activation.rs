#![allow(dead_code)]
use agw_invariants::{Form, Rule, activation::*, store::Controls};
pub fn scope() -> Scope {
    Scope {
        tenant: "t".into(),
        connection: "c".into(),
        vendor: "v".into(),
        operation: "o".into(),
        contract: 1,
    }
}
pub fn candidate() -> Candidate {
    Candidate {
        scope: scope(),
        version: 1,
        rules: vec![Rule {
            id: "range".into(),
            form: Form::Range {
                field: "/n".into(),
                min: 0,
                max: 10,
            },
        }],
        peers: vec![],
        bindings: vec!["/n".into()],
    }
}
pub fn policy() -> Policy {
    Policy {
        scope: scope(),
        revision: 1,
        expires: 100,
        families: vec![Family::Range],
        stages: vec![Stage::Shadow, Stage::Reads, Stage::Blocking],
        modes: vec![
            RolloutMode::Observe,
            RolloutMode::Repair,
            RolloutMode::Strict,
        ],
        versions: (1, 3),
        thresholds: Thresholds {
            minimum_valid: 10,
            minimum_invalid: 10,
            coverage_ppm: 1_000_000,
            false_positive_ppm: 0,
            missed_violation_ppm: 0,
        },
        attestations: vec![Attestation {
            rule: candidate().rules.remove(0),
            semantics: "business limits explicitly approved".into(),
        }],
        peer_attestations: vec![],
        calibration: None,
    }
}
pub fn receipt(id: u64, stage: Stage) -> Receipt {
    Receipt {
        id,
        scope: scope(),
        candidate: 1,
        policy: 1,
        stage,
        training: "train".into(),
        holdout: format!("holdout-{id}"),
        independent: true,
        complete: true,
        started: if stage == Stage::Shadow { 1 } else { 11 },
        finished: if stage == Stage::Shadow { 5 } else { 15 },
        expires: 100,
        counts: Counts {
            eligible: 20,
            evaluated: 20,
            valid: 10,
            invalid: 10,
            false_positives: 0,
            missed_violations: 0,
        },
    }
}
pub fn authorized() -> Result<Controller, Error> {
    let mut c = Controller::new(scope(), 32)?;
    c.configure(Some(policy()), Controls::default())?;
    Ok(c)
}
