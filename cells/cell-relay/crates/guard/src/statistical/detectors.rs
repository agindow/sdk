use crate::availability::EngineFault;
use agw_diff::{Change, DriftClass, Kind, Reason, Side};
use agw_stats::{control, enumeration, field, significance as sig, unit, windows::Window};
use std::panic::RefUnwindSafe;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Detector {
    Unit,
    Enumeration,
    Completeness,
    TypeConsistency,
    Charset,
}
#[derive(Debug)]
pub enum Evidence {
    Unit(sig::Report<unit::Detection>),
    Enumeration(sig::Report<enumeration::Report>),
    Field(Box<sig::Report<field::Report>>),
}
impl Evidence {
    pub fn tests(&self) -> &[sig::Test] {
        match self {
            Self::Unit(r) => r.tests(),
            Self::Enumeration(r) => r.tests(),
            Self::Field(r) => r.tests(),
        }
    }
    pub fn sufficient(&self) -> bool {
        match self {
            Self::Unit(r) => r.sufficient(),
            Self::Enumeration(r) => r.sufficient(),
            Self::Field(r) => r.sufficient(),
        }
    }
}
/// Injection seam for detector fault tests. Production uses only `Builtins`;
/// implementations must do local bounded work and never perform I/O.
pub trait Detectors: RefUnwindSafe {
    /// Parser fault seam. The default lossless parser describes the supplied
    /// captured bytes; implementations must never substitute a document.
    fn inspect(&self, bytes: &[u8]) -> agw_inspector::Inspection {
        agw_inspector::inspect(bytes, false, None)
    }

    fn run(
        &self,
        detector: Detector,
        path: &str,
        baseline: &Window,
        live: &Window,
        family: usize,
    ) -> Result<Evidence, EngineFault>;
}
pub struct Builtins;
impl Detectors for Builtins {
    fn run(
        &self,
        detector: Detector,
        path: &str,
        baseline: &Window,
        live: &Window,
        family: usize,
    ) -> Result<Evidence, EngineFault> {
        let result = match detector {
            Detector::Unit => sig::unit(
                path,
                &baseline.unit().ok_or(EngineFault::Internal)?,
                &live.unit().ok_or(EngineFault::Internal)?,
                family,
            )
            .map(Evidence::Unit),
            Detector::Enumeration => {
                let a = baseline.enumeration();
                let raw: Vec<_> = a.iter().map(|o| o.value).collect();
                sig::enumeration(path, &raw, &live.enumeration(), family).map(Evidence::Enumeration)
            }
            Detector::Completeness => {
                sig::completeness(path, &baseline.fields(), &live.fields(), family)
                    .map(|r| Evidence::Field(Box::new(r)))
            }
            Detector::TypeConsistency => {
                sig::coercion(path, &baseline.fields(), &live.fields(), family)
                    .map(|r| Evidence::Field(Box::new(r)))
            }
            Detector::Charset => sig::charset(path, &baseline.fields(), &live.fields(), family)
                .map(|r| Evidence::Field(Box::new(r))),
        };
        result.map_err(|error| match error {
            sig::Error::ResourceLimit => EngineFault::ResourceExhausted,
            _ => EngineFault::Internal,
        })
    }
}

#[derive(Debug)]
pub struct Record {
    pub side: Side,
    pub path: String,
    pub generation: String,
    pub detector: Detector,
    pub evidence: Evidence,
    /// Includes non-findings and suppressed findings, aligned with tests().
    pub eligibility: Vec<Option<control::Decision>>,
}
impl Record {
    pub(super) fn changes(&self) -> impl Iterator<Item = Change> + '_ {
        self.eligibility
            .iter()
            .filter(|d| d.is_some_and(|v| v.eligible()))
            .map(|_| {
                let (kind, reason) = match self.detector {
                    Detector::Unit => (Kind::UnitsChanged, Reason::UnitsChanged),
                    Detector::Enumeration => (Kind::EnumExpanded, Reason::EnumExpanded),
                    Detector::Completeness => (Kind::NullMeaningChanged, Reason::MeaningChanged),
                    Detector::TypeConsistency => (Kind::TypeChanged, Reason::MeaningChanged),
                    Detector::Charset => (Kind::TypeChanged, Reason::MeaningChanged),
                };
                Change {
                    side: self.side,
                    path: self.path.clone(),
                    to_path: None,
                    kind,
                    class: DriftClass::Semantic,
                    reason,
                }
            })
    }
}
