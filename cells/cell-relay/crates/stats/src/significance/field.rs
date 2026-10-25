use super::{Error, Hypothesis, Report, Test, fisher, report};
use crate::{control::Signal, field as detector};
use agw_inspector::{Number, Value};
use detector::{CharacterClass, Format, Reason, Sentinel, Type};

#[derive(Clone, Copy)]
enum Kind {
    Completeness,
    Type,
    Charset,
}
fn compare(
    path: &str,
    baseline: &[Option<&Value>],
    observed: &[Option<&Value>],
    total_tests: usize,
    kind: Kind,
) -> Result<Report<detector::Report>, Error> {
    let features = match kind {
        Kind::Completeness => 5,
        Kind::Type => 6,
        Kind::Charset => 11,
    };
    report::family(total_tests, features)?;
    if baseline.len() > 4096 || observed.len() > 4096 {
        return Err(Error::ResourceLimit);
    }
    // Borrow long numeric lexemes for the preflight rather than cloning them.
    report::text(
        path,
        baseline.iter().chain(observed).filter_map(|v| match v {
            Some(Value::String(s)) => Some(s.len()),
            Some(Value::Number(Number::Verbatim(s) | Number::Unrepresentable(s))) => Some(s.len()),
            Some(Value::Number(n)) => Some(n.lexeme_len()),
            _ => None,
        }),
    )?;
    let detection = match kind {
        Kind::Completeness => detector::completeness(path, baseline, observed),
        Kind::Type => detector::coercion(path, baseline, observed),
        Kind::Charset => detector::charset(path, baseline, observed),
    }
    .map_err(Error::Field)?;
    let mut tests = Vec::new();
    if detection.sufficient() {
        let (a, b) = (detection.baseline(), detection.observed());
        let reasons = detection.reasons();
        let mut add = |hypothesis, signal, ac, na, bc, nb, finding| -> Result<(), Error> {
            tests.push(Test::new(
                hypothesis,
                fisher(ac, na, bc, nb)?,
                signal,
                total_tests,
                finding,
            ));
            Ok(())
        };
        match kind {
            Kind::Completeness => {
                let (na, nb) = (a.opportunities(), b.opportunities());
                add(
                    Hypothesis::Completeness,
                    Signal::Completeness,
                    a.non_null(),
                    na,
                    b.non_null(),
                    nb,
                    reasons
                        .iter()
                        .any(|r| matches!(r, Reason::CompletenessDelta { .. })),
                )?;
                for sentinel in Sentinel::ALL {
                    add(Hypothesis::Sentinel(sentinel), Signal::Completeness, a.sentinel_count(sentinel), na,
                        b.sentinel_count(sentinel), nb, reasons.iter().any(|r|
                            matches!(r, Reason::SentinelShift { sentinel: s, .. } if *s == sentinel)))?;
                }
            }
            Kind::Type => {
                for ty in Type::NON_NULL {
                    add(
                        Hypothesis::Type(ty),
                        Signal::TypeConsistency,
                        a.type_count(ty),
                        a.non_null(),
                        b.type_count(ty),
                        b.non_null(),
                        reasons.iter().any(
                            |r| matches!(r, Reason::TypeConsistency { kind, .. } if *kind == ty),
                        ),
                    )?;
                }
            }
            Kind::Charset => {
                let (af, bf) = (
                    a.fingerprint().ok_or(Error::InvalidInput)?,
                    b.fingerprint().ok_or(Error::InvalidInput)?,
                );
                let (na, nb) = (a.type_count(Type::String), b.type_count(Type::String));
                for class in CharacterClass::ALL {
                    add(
                        Hypothesis::Character(class),
                        Signal::StringFormat,
                        af.class_count(class),
                        na,
                        bf.class_count(class),
                        nb,
                        reasons.iter().any(
                            |r| matches!(r, Reason::CharsetShift { class: c, .. } if *c == class),
                        ),
                    )?;
                }
                for format in Format::KNOWN {
                    add(Hypothesis::Format(format), Signal::StringFormat, af.format_count(format), na,
                        bf.format_count(format), nb, reasons.iter().any(|r|
                            matches!(r, Reason::FormatEmerged { format: f, .. } if *f == format)))?;
                }
            }
        }
    }
    let sufficient = detection.sufficient();
    Ok(Report::new(path, detection, sufficient, tests))
}
pub fn completeness(
    path: &str,
    baseline: &[Option<&Value>],
    observed: &[Option<&Value>],
    total_tests: usize,
) -> Result<Report<detector::Report>, Error> {
    compare(path, baseline, observed, total_tests, Kind::Completeness)
}
pub fn coercion(
    path: &str,
    baseline: &[Option<&Value>],
    observed: &[Option<&Value>],
    total_tests: usize,
) -> Result<Report<detector::Report>, Error> {
    compare(path, baseline, observed, total_tests, Kind::Type)
}
pub fn charset(
    path: &str,
    baseline: &[Option<&Value>],
    observed: &[Option<&Value>],
    total_tests: usize,
) -> Result<Report<detector::Report>, Error> {
    compare(path, baseline, observed, total_tests, Kind::Charset)
}
