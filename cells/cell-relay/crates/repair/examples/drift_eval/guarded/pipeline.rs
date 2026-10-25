use super::super::{arms, model::Task};
use super::{
    Fault,
    facts::{Facts, verify},
};
use agw_diff::Side;
use agw_guard::{
    availability::{Budget, EngineFault},
    gated::{Evidence, Pipeline},
};
use serde_json::Value;
use std::cell::RefCell;

pub struct Engine<'a> {
    pub class: &'a str,
    pub task: &'a Task,
    pub verifier: Verifier<'a>,
}
/// The verifier cannot access proposals, confidence, gold answers or labels.
pub struct Verifier<'a> {
    pub original: &'a Value,
    pub facts: &'a Facts,
    pub fault: Fault,
    pub results: RefCell<Vec<String>>,
    pub input: RefCell<Option<(Vec<u8>, Vec<usize>)>>,
}
impl Verifier<'_> {
    fn check(
        &self,
        patched: &[u8],
        applied: &[usize],
        work: &mut Budget,
    ) -> Result<bool, EngineFault> {
        work.charge(patched.len().saturating_add(applied.len()))?;
        *self.input.borrow_mut() = Some((patched.to_vec(), applied.to_vec()));
        self.results.borrow_mut().push("invoked".into());
        match self.fault {
            Fault::Error => {
                self.results.borrow_mut().push("error".into());
                return Err(EngineFault::Internal);
            }
            Fault::Panic => {
                self.results.borrow_mut().push("panic".into());
                panic!("injected verifier panic");
            }
            Fault::Budget => {
                self.results.borrow_mut().push("budget".into());
                work.charge(usize::MAX)?;
            }
            _ => {}
        }
        let passed =
            self.fault != Fault::Refuse && verify(self.original, self.facts, patched, applied);
        // Invocation is represented by one terminal result in successful calls.
        let mut results = self.results.borrow_mut();
        results.pop();
        results.push(if passed { "passed" } else { "refused" }.into());
        Ok(passed)
    }
}
impl Pipeline for Engine<'_> {
    fn detect(&self, side: Side, _: &[u8], work: &mut Budget) -> Result<Evidence, EngineFault> {
        work.charge(1)?;
        if side == Side::Response {
            return Ok(Evidence::default());
        }
        let (report, candidates) = arms::evidence(self.class, self.task);
        Ok(Evidence { report, candidates })
    }
    fn verify(
        &self,
        _: Side,
        patched: &[u8],
        applied: &[usize],
        work: &mut Budget,
    ) -> Result<bool, EngineFault> {
        self.verifier.check(patched, applied, work)
    }
}
