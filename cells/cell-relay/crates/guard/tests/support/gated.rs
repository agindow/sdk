#![allow(dead_code)]
use agw_audit::contracts::audit_artifact::AuditArtifact;
use agw_diff::{Change, DriftClass, Kind, Reason, Report, Side};
use agw_guard::{
    availability::{Budget, EngineFault},
    gated::{Evidence, Pipeline},
};
use agw_repair::{Candidate, Operation};
use std::sync::atomic::{AtomicUsize, Ordering};

pub fn draft() -> Result<AuditArtifact, Box<dyn std::error::Error>> {
    serde_json::from_str(include_str!(
        "../../../../../../contracts/examples/audit_artifact.valid.json"
    ))
    .map_err(Into::into)
}
pub struct Engine {
    pub class: Option<DriftClass>,
    pub verify: bool,
    pub confidence: f64,
    pub propose: bool,
    pub fault: Option<Side>,
    pub panic: bool,
    pub evaluations: AtomicUsize,
    pub verifications: AtomicUsize,
}
impl Engine {
    pub fn new(class: Option<DriftClass>) -> Self {
        Self {
            class,
            verify: true,
            confidence: 1.0,
            propose: false,
            fault: None,
            panic: false,
            evaluations: AtomicUsize::new(0),
            verifications: AtomicUsize::new(0),
        }
    }
}
impl Pipeline for Engine {
    fn detect(&self, side: Side, _: &[u8], budget: &mut Budget) -> Result<Evidence, EngineFault> {
        budget.charge(1)?;
        self.evaluations.fetch_add(1, Ordering::SeqCst);
        if self.fault == Some(side) {
            assert!(!self.panic, "injected engine panic");
            return Err(EngineFault::Internal);
        }
        Ok(Evidence {
            report: Report {
                changes: self
                    .class
                    .map(|class| Change {
                        side,
                        path: "old".into(),
                        to_path: (class == DriftClass::Shape).then(|| "new".into()),
                        kind: if class == DriftClass::Shape {
                            Kind::Renamed
                        } else {
                            Kind::UnitsChanged
                        },
                        class,
                        reason: if class == DriftClass::Shape {
                            Reason::ConfirmedIdentity
                        } else {
                            Reason::UnitsChanged
                        },
                    })
                    .into_iter()
                    .collect(),
                skipped: None,
            },
            candidates: if self.propose {
                vec![Candidate {
                    finding: 0,
                    operation: if self.class == Some(DriftClass::Shape) {
                        Operation::Rename { to: "new".into() }
                    } else {
                        Operation::Replace(b"2".to_vec())
                    },
                    evidence: agw_repair::Evidence {
                        detector: self.confidence,
                        applicability: 1.0,
                    },
                }]
            } else {
                vec![]
            },
        })
    }
    fn verify(
        &self,
        _: Side,
        _: &[u8],
        _: &[usize],
        budget: &mut Budget,
    ) -> Result<bool, EngineFault> {
        budget.charge(1)?;
        self.verifications.fetch_add(1, Ordering::SeqCst);
        Ok(self.verify)
    }
}
