//! TI-24's pure policy matrix; availability remains the guard's first layer.
//! No detector, transport, repair execution or inference occurs here.
pub use agw_audit::contracts::audit_artifact::{
    AuditArtifactMode as Mode, AuditArtifactOperation as Operation,
};
pub use agw_diff::DriftClass as Class;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Repair {
    Verified,
    Failed,
    Unavailable,
}
/// Ordering combines independent dispatched decisions: any halt dominates,
/// then degradation, verified repair, and clean/observe pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Decision {
    Pass,
    Repaired,
    Degraded,
    Halt,
}
/// Only verified repairs discharge a finding. Availability faults never enter
/// this function, and statistical hypotheses cannot supply `Verified`.
pub fn decide(mode: Mode, operation: Operation, class: Option<Class>, repair: Repair) -> Decision {
    decide_with_protection(mode, operation, class, repair, true)
}

/// Explicit TI-26 opt-out. Semantic writes and strict mode are non-overridable.
/// The existing default shape passthrough rule remains unchanged.
pub fn decide_with_protection(
    mode: Mode,
    operation: Operation,
    class: Option<Class>,
    repair: Repair,
    fail_closed: bool,
) -> Decision {
    if mode == Mode::Observe || class.is_none() {
        Decision::Pass
    } else if repair == Repair::Verified {
        Decision::Repaired
    } else if mode == Mode::Strict
        || (operation != Operation::Read
            && (class == Some(Class::Semantic) || (fail_closed && class == Some(Class::Unknown))))
    {
        Decision::Halt
    } else {
        Decision::Degraded
    }
}
