//! v3 explicit guarded qualification; the v2 seam remains a separate diagnostic.
pub mod diagnostics;
pub mod facts;
mod pipeline;
mod regression;
pub fn compare(current: &Report, previous: &Report) -> Result<(), Error> {
    regression::compare(current, previous)
}
use super::{
    Error, Label, Report as SeamReport, fixtures,
    model::{Case, Metrics, Task},
    score, scoring,
};
use agw_guard::{
    config::{Config, RuntimeConfig},
    gated::{Body, Call},
    readiness::AvailabilityGate,
};
use facts::{Declaration, Facts};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Fault {
    Healthy,
    Refuse,
    Error,
    Panic,
    NotReady,
    Budget,
}
pub const FAULTS: [Fault; 5] = [
    Fault::Refuse,
    Fault::Error,
    Fault::Panic,
    Fault::NotReady,
    Fault::Budget,
];
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    pub case: Case,
    pub facts: Facts,
    pub fault: Fault,
    pub verification: Vec<String>,
    pub verification_input: Option<(Vec<u8>, Vec<usize>)>,
    pub decision: String,
    pub dispatch_count: usize,
    pub bytes: Option<Vec<u8>>,
    pub bypasses: u8,
    pub original_bytes: bool,
    pub original_outcome: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerificationMetrics {
    pub invoked: usize,
    pub passed: usize,
    pub refused: usize,
    pub faulted: usize,
    pub not_run: usize,
    pub bypasses: usize,
    pub dispatches: usize,
}
pub fn verification_metrics(records: &[Record]) -> VerificationMetrics {
    let count = |result| {
        records
            .iter()
            .filter(|r| r.verification.iter().any(|s| s == result))
            .count()
    };
    let invoked = records
        .iter()
        .filter(|r| r.verification_input.is_some())
        .count();
    let passed = count("passed");
    let refused = count("refused");
    VerificationMetrics {
        invoked,
        passed,
        refused,
        faulted: invoked - passed - refused,
        not_run: records.len() - invoked,
        bypasses: records.iter().map(|r| usize::from(r.bypasses)).sum(),
        dispatches: records.iter().map(|r| r.dispatch_count).sum(),
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Report {
    pub version: u32,
    pub profile: String,
    pub declaration: String,
    pub seam: SeamReport,
    pub healthy: Vec<Record>,
    pub faults: Vec<Record>,
    pub verification_metrics: VerificationMetrics,
    pub fault_verification_metrics: VerificationMetrics,
    pub metrics: Vec<Metrics>,
    pub fault_metrics: Vec<Metrics>,
}

pub fn execute(class: &str, task: &Task, facts: &Facts, fault: Fault) -> Result<Record, Error> {
    let source = serde_json::to_vec(&task.input)?;
    let runtime = RuntimeConfig::from_sources(
        Config {
            mode: Some("repair".into()),
            ..Config::default()
        },
        None,
    )?;
    let gate = AvailabilityGate::new(fault != Fault::NotReady);
    let engine = pipeline::Engine {
        class,
        task,
        verifier: pipeline::Verifier {
            original: &task.input,
            facts,
            fault,
            results: RefCell::new(Vec::new()),
            input: RefCell::new(None),
        },
    };
    let mut draft: agw_audit::contracts::audit_artifact::AuditArtifact = serde_json::from_str(
        include_str!("../../../../../../../contracts/examples/audit_artifact.valid.json"),
    )?;
    draft.operation = agw_policy::Operation::Write;
    let mut bytes = None;
    let mut dispatch_count = 0;
    let original = Box::new(73);
    let pointer = &*original as *const i32;
    let mut original_bytes = false;
    let result = Call::new(runtime.begin_call(None)?, &gate, 100_000).execute(
        Body::complete(&source),
        &engine,
        draft,
        |sent| {
            original_bytes = std::ptr::eq(sent, source.as_slice());
            dispatch_count += 1;
            bytes = Some(sent.to_vec());
            (original, Body::complete(b"{}"))
        },
    );
    let original_outcome = result
        .outcome
        .as_ref()
        .is_some_and(|v| std::ptr::eq(&**v, pointer));
    let sent = bytes
        .as_ref()
        .map(|b| serde_json::from_slice(b))
        .transpose()?;
    // The scorer supplies identity/labels only after execution; no oracle enters Pipeline.
    let fixture = fixtures()?
        .into_iter()
        .find(|f| f.class == class && f.task.input == task.input)
        .ok_or("unknown task")?;
    Ok(Record {
        case: score(
            &fixture,
            "guarded",
            sent,
            vec![format!("reason={}", result.request.reason)],
        ),
        facts: facts.clone(),
        fault,
        verification: engine.verifier.results.into_inner(),
        verification_input: engine.verifier.input.into_inner(),
        decision: format!("{:?}", result.request.decision),
        dispatch_count,
        bytes,
        bypasses: result.bypasses,
        original_bytes,
        original_outcome,
    })
}

pub fn outcome_metrics(records: &[Record]) -> Vec<Metrics> {
    let cases: Vec<_> = records.iter().map(|r| r.case.clone()).collect();
    super::CLASSES
        .into_iter()
        .chain(["all"])
        .map(|class| scoring::metric(&cases, class, "all", "guarded"))
        .chain(
            [Label::Repairable, Label::Ambiguous, Label::Unsupported]
                .into_iter()
                .map(|label| scoring::metric(&cases, "all", label.name(), "guarded")),
        )
        .collect()
}
pub fn run() -> Result<Report, Error> {
    let declaration: Declaration = serde_json::from_str(facts::DECLARATION)?;
    let fixtures = fixtures()?;
    if declaration.version != 1 || declaration.cases.len() != fixtures.len() {
        return Err("incompatible verification declaration".into());
    }
    let mut healthy = Vec::new();
    let mut faults = Vec::new();
    for f in fixtures {
        let facts = declaration
            .cases
            .get(&f.id)
            .ok_or("missing verification facts")?;
        healthy.push(execute(&f.class, &f.task, facts, Fault::Healthy)?);
        // Same identities and labels, reported separately for every fault mode.
        for fault in FAULTS {
            faults.push(execute(&f.class, &f.task, facts, fault)?);
        }
    }
    Ok(Report {
        version: 3,
        profile: "guarded-v3".into(),
        declaration: facts::DECLARATION.into(),
        seam: super::run()?,
        verification_metrics: verification_metrics(&healthy),
        fault_verification_metrics: verification_metrics(&faults),
        metrics: outcome_metrics(&healthy),
        fault_metrics: outcome_metrics(&faults),
        healthy,
        faults,
    })
}
