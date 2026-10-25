//! TI-17 bounded field evidence; see runtime contract §10.21.
//! No field extraction, payload rewriting, I/O, policy or guard wiring.
mod fingerprint;
mod profile;
use agw_diff::DriftClass;
use agw_inspector::Value;
pub use fingerprint::{CharacterClass, Fingerprint, Format};
pub use profile::{Profile, profile};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    InvalidInput,
    ResourceLimit,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Type {
    Null,
    Boolean,
    Integer,
    Number,
    String,
    Array,
    Object,
}
impl Type {
    pub(crate) const NON_NULL: [Self; 6] = [
        Self::Boolean,
        Self::Integer,
        Self::Number,
        Self::String,
        Self::Array,
        Self::Object,
    ];
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sentinel {
    Missing,
    Null,
    Zero,
    EmptyString,
}
impl Sentinel {
    pub(crate) const ALL: [Self; 4] = [Self::Missing, Self::Null, Self::Zero, Self::EmptyString];
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rates {
    pub baseline: f64,
    pub observed: f64,
}
impl Rates {
    fn new(a: usize, na: usize, b: usize, nb: usize) -> Self {
        Self {
            baseline: a as f64 / na as f64,
            observed: b as f64 / nb as f64,
        }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub enum Reason {
    CompletenessDelta {
        rates: Rates,
    },
    SentinelShift {
        sentinel: Sentinel,
        rates: Rates,
    },
    TypeConsistency {
        kind: Type,
        rates: Rates,
    },
    CharsetShift {
        class: CharacterClass,
        rates: Rates,
    },
    FormatEmerged {
        format: Format,
        pattern: &'static str,
        rates: Rates,
    },
}
#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    path: String,
    baseline: Profile,
    observed: Profile,
    sufficient: bool,
    reasons: Vec<Reason>,
}
impl Report {
    pub fn path(&self) -> &str {
        &self.path
    }
    pub fn baseline(&self) -> &Profile {
        &self.baseline
    }
    pub fn observed(&self) -> &Profile {
        &self.observed
    }
    pub fn sufficient(&self) -> bool {
        self.sufficient
    }
    pub fn reasons(&self) -> &[Reason] {
        &self.reasons
    }
    pub fn class(&self) -> Option<DriftClass> {
        (!self.reasons.is_empty()).then_some(DriftClass::Semantic)
    }
}
#[derive(Clone, Copy)]
enum Detector {
    Completeness,
    Coercion,
    Charset,
}
// Integer cross-products keep inclusive boundaries exact. Window caps keep
// these products bounded even on 32-bit platforms.
fn changed(a: usize, na: usize, b: usize, nb: usize) -> bool {
    (a * nb).abs_diff(b * na) * 5 >= na * nb
}
fn compare(
    path: &str,
    baseline: &[Option<&Value>],
    observed: &[Option<&Value>],
    detector: Detector,
) -> Result<Report, Error> {
    if path.is_empty() {
        return Err(Error::InvalidInput);
    }
    let mut text = 0;
    profile::text_budget(path, &mut text)?;
    let a = profile::build(baseline, matches!(detector, Detector::Charset), &mut text)?;
    let b = profile::build(observed, matches!(detector, Detector::Charset), &mut text)?;
    let (na, nb) = match detector {
        Detector::Completeness => (a.opportunities(), b.opportunities()),
        Detector::Coercion => (a.non_null(), b.non_null()),
        Detector::Charset => (a.type_count(Type::String), b.type_count(Type::String)),
    };
    let sufficient = na >= 20 && nb >= 20;
    let mut reasons = Vec::new();
    if sufficient {
        match detector {
            Detector::Completeness => {
                if changed(a.non_null(), na, b.non_null(), nb) {
                    reasons.push(Reason::CompletenessDelta {
                        rates: Rates::new(a.non_null(), na, b.non_null(), nb),
                    });
                }
                for sentinel in Sentinel::ALL {
                    let (ac, bc) = (a.sentinel_count(sentinel), b.sentinel_count(sentinel));
                    if changed(ac, na, bc, nb) {
                        reasons.push(Reason::SentinelShift {
                            sentinel,
                            rates: Rates::new(ac, na, bc, nb),
                        });
                    }
                }
            }
            Detector::Coercion => {
                for kind in Type::NON_NULL {
                    let (ac, bc) = (a.type_count(kind), b.type_count(kind));
                    if ac * 10 >= na * 9 && ac * nb > bc * na && changed(ac, na, bc, nb) {
                        reasons.push(Reason::TypeConsistency {
                            kind,
                            rates: Rates::new(ac, na, bc, nb),
                        });
                    }
                }
            }
            Detector::Charset => {
                if let (Some(af), Some(bf)) = (a.fingerprint(), b.fingerprint()) {
                    for class in CharacterClass::ALL {
                        let (ac, bc) = (af.class_count(class), bf.class_count(class));
                        if changed(ac, na, bc, nb) {
                            reasons.push(Reason::CharsetShift {
                                class,
                                rates: Rates::new(ac, na, bc, nb),
                            });
                        }
                    }
                    for format in Format::KNOWN {
                        let (ac, bc) = (af.format_count(format), bf.format_count(format));
                        if ac * 20 <= na
                            && bc * 5 >= nb
                            && let Some(pattern) = format.pattern()
                        {
                            reasons.push(Reason::FormatEmerged {
                                format,
                                pattern,
                                rates: Rates::new(ac, na, bc, nb),
                            });
                        }
                    }
                }
            }
        }
    }
    Ok(Report {
        path: path.into(),
        baseline: a,
        observed: b,
        sufficient,
        reasons,
    })
}

pub fn completeness(
    path: &str,
    baseline: &[Option<&Value>],
    observed: &[Option<&Value>],
) -> Result<Report, Error> {
    compare(path, baseline, observed, Detector::Completeness)
}
pub fn coercion(
    path: &str,
    baseline: &[Option<&Value>],
    observed: &[Option<&Value>],
) -> Result<Report, Error> {
    compare(path, baseline, observed, Detector::Coercion)
}
pub fn charset(
    path: &str,
    baseline: &[Option<&Value>],
    observed: &[Option<&Value>],
) -> Result<Report, Error> {
    compare(path, baseline, observed, Detector::Charset)
}
