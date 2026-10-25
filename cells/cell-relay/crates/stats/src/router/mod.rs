//! TI-20 local handler selection, bounded Tier 2 outbox and reviewed accuracy.
//!
//! No I/O, callbacks, LLMs, policy execution or guard wiring. Contract §10.25.
mod cost;
mod delivery;
pub use delivery::DeliveryContext;
mod review;

use agw_diff::{Change, DriftClass, Report, Side, Skip};
pub use cost::{Admission, CallBudget, Grant, Job, Outbox, Receipt};
pub use review::Reviews;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    InvalidInput,
    ResourceLimit,
    BudgetExhausted,
    CounterOverflow,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}

/// Internal labels only. Invariant does not extend the drift-class wire enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Label {
    Shape,
    Semantic,
    Invariant,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Handler {
    Repair,
    Policy,
    Verify,
    Conservative,
}

/// Confirmed caller evidence; this router does not discover or verify invariants.
#[derive(Debug, PartialEq, Eq)]
pub struct Invariant<'a> {
    pub side: Side,
    pub path: &'a str,
    pub rule: &'a str,
}

#[derive(Debug, Clone, Copy)]
pub enum Finding<'a> {
    Diff(&'a Change),
    Invariant(&'a Invariant<'a>),
}

#[derive(Debug)]
pub struct Dispatch<'a> {
    finding: Finding<'a>,
}
impl<'a> Dispatch<'a> {
    pub fn finding(&self) -> Finding<'a> {
        self.finding
    }
    pub fn label(&self) -> Label {
        match self.finding {
            Finding::Diff(c) => match c.class {
                DriftClass::Shape => Label::Shape,
                DriftClass::Semantic => Label::Semantic,
                DriftClass::Unknown => Label::Unknown,
            },
            Finding::Invariant(_) => Label::Invariant,
        }
    }
    pub fn handler(&self) -> Handler {
        match self.label() {
            Label::Shape => Handler::Repair,
            Label::Semantic => Handler::Policy,
            Label::Invariant => Handler::Verify,
            Label::Unknown => Handler::Conservative,
        }
    }
}

#[derive(Debug)]
pub struct Routing<'a> {
    findings: Vec<Dispatch<'a>>,
    unavailable: Option<Skip>,
}
impl<'a> Routing<'a> {
    pub fn findings(&self) -> &[Dispatch<'a>] {
        &self.findings
    }
    pub fn unavailable(&self) -> Option<Skip> {
        self.unavailable
    }
}

/// Validate all metadata before allocating; retain borrowed evidence verbatim.
/// Skipped reports cannot masquerade as clean observations or investigation work.
pub fn route<'a>(
    report: &'a Report,
    invariants: &'a [Invariant<'a>],
) -> Result<Routing<'a>, Error> {
    if let Some(skip) = report.skipped {
        return Ok(Routing {
            findings: Vec::new(),
            unavailable: Some(skip),
        });
    }
    let count = report
        .changes
        .len()
        .checked_add(invariants.len())
        .ok_or(Error::ResourceLimit)?;
    if count > 4096 {
        return Err(Error::ResourceLimit);
    }
    let mut bytes = 0usize;
    for change in &report.changes {
        bytes += text(&change.path)?;
        if let Some(to) = &change.to_path {
            bytes += text(to)?;
        }
    }
    for invariant in invariants {
        bytes += text(invariant.path)? + text(invariant.rule)?;
    }
    if bytes > 1024 * 1024 {
        return Err(Error::ResourceLimit);
    }
    let findings = report
        .changes
        .iter()
        .map(Finding::Diff)
        .chain(invariants.iter().map(Finding::Invariant))
        .map(|finding| Dispatch { finding })
        .collect();
    Ok(Routing {
        findings,
        unavailable: None,
    })
}

fn text(value: &str) -> Result<usize, Error> {
    if value.is_empty() {
        Err(Error::InvalidInput)
    } else if value.len() > 4096 {
        Err(Error::ResourceLimit)
    } else {
        Ok(value.len())
    }
}
