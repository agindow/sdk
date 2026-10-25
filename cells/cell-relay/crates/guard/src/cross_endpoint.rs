//! TI-23: compare current exact values with frozen same-workflow peer evidence.
use crate::{
    availability::EngineFault,
    invariant::{Record, decode_path, scalar},
    statistical::{Body, InvalidPlan, Phase, Plan},
};
use agw_inspector::{Inspection, Parse};
use agw_invariants::cross_endpoint::{
    self as cross, Observation, Reason, Rule, Scope, Side, Snapshot,
};
use agw_invariants::{Form, Program, Rule as LocalRule, Scalar, Verdict, compile};
use serde_json::json;
#[derive(Debug, Clone)]
struct Prepared {
    rule: Rule,
    left: Vec<String>,
    right: Vec<String>,
    program: Program,
}
#[derive(Debug, Clone)]
pub struct Selection {
    pub(crate) version: u64,
    pub(crate) contract_version: u64,
    rules: Vec<Prepared>,
    pub(crate) activation: Option<std::sync::Arc<agw_invariants::activation::Generation>>,
}
impl Selection {
    pub fn new(version: u64, contract_version: u64, rules: Vec<Rule>) -> Result<Self, InvalidPlan> {
        if version == 0 || contract_version == 0 || rules.len() > cross::MAX_RULES {
            return Err(InvalidPlan);
        }
        let mut prepared: Vec<Prepared> = Vec::new();
        for rule in rules {
            rule.validate().map_err(|_| InvalidPlan)?;
            if prepared.iter().any(|p| p.rule.id == rule.id) {
                return Err(InvalidPlan);
            }
            let left = decode_path(&rule.left.field)?;
            let right = decode_path(&rule.right.field)?;
            let local = LocalRule {
                id: rule.id.clone(),
                form: Form::CrossEndpoint {
                    left: "left".into(),
                    right: "right".into(),
                    tolerance: rule.tolerance,
                },
            };
            let program = compile(&[local], &["left", "right"]).map_err(|_| InvalidPlan)?;
            prepared.push(Prepared {
                rule,
                left,
                right,
                program,
            });
        }
        Ok(Self {
            version,
            contract_version,
            rules: prepared,
            activation: None,
        })
    }
    pub(crate) fn activated(
        generation: std::sync::Arc<agw_invariants::activation::Generation>,
    ) -> Result<Self, InvalidPlan> {
        let mut selection = Self::new(
            generation.snapshot().version,
            generation.snapshot().scope.contract_version,
            generation.peers().to_vec(),
        )?;
        selection.activation = Some(generation);
        Ok(selection)
    }
    pub(crate) fn mode_for(
        &self,
        rule: &str,
        mode: crate::config::Mode,
        operation: crate::statistical::Operation,
    ) -> Option<crate::config::Mode> {
        self.rules
            .iter()
            .any(|r| r.rule.id == rule)
            .then(|| crate::invariant::stage_mode(self.activation.as_deref(), mode, operation))
    }
    pub(crate) fn allowance(&self) -> usize {
        self.rules.len() * (2 * 4096 * 32 + 256 * 1024)
    }
    pub(crate) fn valid_for(&self, operation: &str) -> bool {
        self.rules
            .iter()
            .all(|p| p.rule.left.operation == operation || p.rule.right.operation == operation)
    }
    pub(crate) fn conflicts(&self, selection: &crate::invariant::Selection) -> bool {
        self.contract_version != selection.snapshot.scope.contract_version
            || self.rules.iter().any(|p| {
                selection
                    .snapshot
                    .program
                    .rules()
                    .iter()
                    .any(|r| r.id() == p.rule.id)
            })
    }
}
pub(crate) struct Workflow {
    pub snapshot: Result<Snapshot, cross::Error>,
    pub sequence: u64,
}
#[derive(Debug, Clone)]
pub struct Comparison {
    pub id: String,
    pub reason: Reason,
    pub concept: String,
    pub unit: String,
    pub tolerance: u64,
    pub max_age_ms: u64,
    pub max_skew_ms: u64,
    pub left: Option<Observation>,
    pub right: Option<Observation>,
}
fn fault(error: cross::Error) -> EngineFault {
    match error {
        cross::Error::Busy | cross::Error::Poisoned => EngineFault::RegistryUnavailable,
        _ => EngineFault::RegistryCorrupt,
    }
}
#[allow(clippy::too_many_arguments)]
pub(crate) fn evaluate(
    phase: &mut Phase,
    plan: &Plan,
    selection: &Selection,
    workflow: Option<&Workflow>,
    inspection: &Inspection,
    body: Body<'_>,
    side: agw_diff::Side,
) -> Result<(), EngineFault> {
    let Parse::Parsed(root) = &inspection.parse else {
        return Err(EngineFault::Internal);
    };
    let side = if side == agw_diff::Side::Request {
        Side::Request
    } else {
        Side::Response
    };
    let snapshot = if let Some(workflow) = workflow {
        if workflow.sequence == 0 {
            return Err(EngineFault::RegistryCorrupt);
        }
        let snapshot = workflow.snapshot.as_ref().map_err(|e| fault(*e))?;
        let scope = snapshot.scope();
        if scope.tenant != plan.identity.tenant
            || scope.connection != plan.identity.connection
            || scope.vendor != plan.identity.vendor
        {
            return Err(EngineFault::RegistryCorrupt);
        }
        phase.cross_scope = Some(scope.clone());
        Some(snapshot)
    } else {
        None
    };
    for prepared in &selection.rules {
        let rule = &prepared.rule;
        let is_left = rule.left.operation == plan.identity.operation && rule.left.side == side;
        let is_right = rule.right.operation == plan.identity.operation && rule.right.side == side;
        if !is_left && !is_right {
            continue;
        }
        let left_key = rule.key(&rule.left, selection.contract_version);
        let right_key = rule.key(&rule.right, selection.contract_version);
        let mut left = snapshot.and_then(|s| s.find(&left_key)).cloned();
        let mut right = snapshot.and_then(|s| s.find(&right_key)).cloned();
        let mut out_of_order = false;
        if let (Some(snapshot), Some(workflow)) = (snapshot, workflow) {
            let (target, key, path) = if is_left {
                (&mut left, left_key, &prepared.left)
            } else {
                (&mut right, right_key, &prepared.right)
            };
            let current = Observation {
                scope: snapshot.scope().clone(),
                key,
                value: match scalar(root, path) {
                    Scalar::Integer(n) => Some(n),
                    _ => None,
                },
                at_ms: body.at.timestamp_millis(),
                sequence: workflow.sequence,
            };
            out_of_order = target.as_ref().is_some_and(|prior| {
                current.sequence <= prior.sequence || current.at_ms < prior.at_ms
            });
            // Never use an old current value after seeing a new unsupported one.
            *target = Some(current.clone());
            if !out_of_order
                && !phase
                    .cross_observations
                    .iter()
                    .any(|o| o.key == current.key)
            {
                phase.cross_observations.push(current);
            }
        }
        let reason = if out_of_order {
            Reason::OutOfOrder
        } else {
            cross::compare(
                rule,
                left.as_ref(),
                right.as_ref(),
                body.at.timestamp_millis(),
            )
            .map_err(fault)?
        };
        let verdict = match reason {
            Reason::Pass => Verdict::Pass,
            Reason::Divergence => Verdict::Violation,
            _ => Verdict::Unavailable,
        };
        if matches!(reason, Reason::Pass | Reason::Divergence) {
            let values = [
                left.as_ref().and_then(|o| o.value),
                right.as_ref().and_then(|o| o.value),
            ]
            .map(|v| v.map_or(Scalar::Missing, Scalar::Integer));
            if prepared.program.rules()[0].evaluate(&values) != verdict {
                return Err(EngineFault::Internal);
            }
        }
        let observed = left.as_ref().and_then(|o| o.value);
        let expected = right.as_ref().and_then(|o| o.value);
        phase.invariants.push(Record {
            id: rule.id.clone(),
            verdict,
            actual: observed.map(|v| v.to_string()),
            expected: expected.map(|v| format!("Equal {v}; absolute tolerance {}", rule.tolerance)),
        });
        phase.cross_endpoint.push(Comparison {
            id: rule.id.clone(),
            reason,
            concept: rule.concept.clone(),
            unit: rule.unit.clone(),
            tolerance: rule.tolerance,
            max_age_ms: rule.max_age_ms,
            max_skew_ms: rule.max_skew_ms,
            left,
            right,
        });
    }
    Ok(())
}
pub(crate) fn metadata(
    selection: &Selection,
    scope: Option<&Scope>,
    records: &[Comparison],
) -> serde_json::Value {
    let scope = scope.map(|s| json!({"tenant":s.tenant,"connection":s.connection,"vendor":s.vendor,"workflow":s.workflow,"entity":s.entity,"revision":s.revision}));
    let observation = |o: &Option<Observation>| {
        o.as_ref().map(|o| json!({"operation":o.key.endpoint.operation,"side":format!("{:?}",o.key.endpoint.side),"field":o.key.endpoint.field,"value":o.value,"at_ms":o.at_ms,"sequence":o.sequence}))
    };
    let endpoint = |e: &cross::Endpoint| json!({"operation":e.operation,"side":format!("{:?}",e.side),"field":e.field});
    let activation = selection
        .activation
        .as_deref()
        .filter(|_| !records.is_empty())
        .map(crate::invariant::activation::metadata);
    json!({"version":selection.version,"contract_version":selection.contract_version,"scope":scope,"activation":activation,
        "records":records.iter().map(|r| {
            let rule = selection.rules.iter().find(|p| p.rule.id == r.id).map(|p| &p.rule);
            json!({"id":r.id,"reason":format!("{:?}",r.reason),"concept":r.concept,"unit":r.unit,"tolerance":r.tolerance,"max_age_ms":r.max_age_ms,"max_skew_ms":r.max_skew_ms,"left":observation(&r.left),"right":observation(&r.right),"left_source":rule.map(|r| endpoint(&r.left)),"right_source":rule.map(|r| endpoint(&r.right))})
        }).collect::<Vec<_>>()})
}
