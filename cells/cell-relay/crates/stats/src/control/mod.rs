//! TI-19 local alert controls. No I/O, detector math, policy or guard wiring.
//! Runtime contract §10.23 defines the caller's evidence/provenance obligations.
mod budget;
mod materiality;
pub mod replay;
mod selection;

pub use budget::{AlertAction, AlertBudget};
pub use materiality::{Decision, Evidence, Floors, Override, Ranked, Signal};
pub use selection::{Scope, Season, Side, Window, select};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    InvalidInput,
    ResourceLimit,
    StaleDay,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}

fn text(value: &str) -> Result<(), Error> {
    if value.is_empty() {
        Err(Error::InvalidInput)
    } else if value.len() > 1024 {
        Err(Error::ResourceLimit)
    } else {
        Ok(())
    }
}
