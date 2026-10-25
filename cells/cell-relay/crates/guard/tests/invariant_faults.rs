#[path = "support/invariants.rs"]
mod fixture;
#[path = "support/statistical.rs"]
mod stats;
use agw_guard::{
    availability::EngineFault,
    statistical::{Builtins, Detector, Detectors, Evidence, Operation},
};
use agw_inspector::{Inspection, Parse, ParseClass, ParseFailure};
use agw_stats::windows::Window;
use fixture::{CLEAN, Error};
use std::sync::atomic::{AtomicUsize, Ordering};
struct Parser {
    calls: AtomicUsize,
    fault_on_response: bool,
    panic: bool,
}
impl Detectors for Parser {
    fn inspect(&self, bytes: &[u8]) -> Inspection {
        let count = self.calls.fetch_add(1, Ordering::Relaxed);
        if !self.fault_on_response || count % 2 == 1 {
            if self.panic {
                panic!("injected parser failure");
            }
            return Inspection {
                truncated: false,
                parse: Parse::Unparseable(ParseFailure {
                    class: ParseClass::Data,
                    line: 0,
                    column: 0,
                    message: "internal".into(),
                }),
            };
        }
        Builtins.inspect(bytes)
    }
    fn run(
        &self,
        _: Detector,
        _: &str,
        _: &Window,
        _: &Window,
        _: usize,
    ) -> Result<Evidence, EngineFault> {
        panic!("no statistical fields selected")
    }
}
#[test]
fn parser_faults_and_panics_preserve_error_identity_and_bypass_once() -> Result<(), Error> {
    for mode in ["observe", "repair", "strict"] {
        for response in [false, true] {
            for panic in [false, true] {
                let parser = Parser {
                    calls: AtomicUsize::new(0),
                    fault_on_response: response,
                    panic,
                };
                let error = Box::new("upstream error");
                let pointer = &*error as *const &str;
                let calls = std::cell::Cell::new(0);
                let output = fixture::engine()?
                    .begin_call(
                        stats::context(mode)?,
                        Operation::Write,
                        "audit",
                        100_000_000,
                        0,
                    )
                    .execute_with(stats::body(CLEAN), None, &parser, || {
                        calls.set(calls.get() + 1);
                        (Err::<(), _>(error), stats::body(CLEAN))
                    });
                assert_eq!(output.bypasses, 1);
                assert_eq!(calls.get(), 1);
                assert_eq!(
                    parser.calls.load(Ordering::Relaxed),
                    if response { 2 } else { 1 }
                );
                let kept = output.outcome.ok_or("outcome")?.err().ok_or("error")?;
                assert_eq!(&*kept as *const &str, pointer);
                let faulted = if response {
                    output.response.ok_or("response")?
                } else {
                    assert!(output.response.is_none());
                    output.request
                };
                assert!(faulted.invariants.is_empty());
                assert!(faulted.invariant_audit.is_none());
                assert!(faulted.fault.is_some());
            }
        }
    }
    Ok(())
}
#[test]
fn readiness_budget_and_shared_response_cooldown_skip_actual_evaluation() -> Result<(), Error> {
    let engine = fixture::engine()?;
    let parser = Parser {
        calls: AtomicUsize::new(0),
        fault_on_response: true,
        panic: false,
    };
    engine.set_ready(false);
    let not_ready = engine
        .begin_call(
            stats::context("strict")?,
            Operation::Write,
            "audit",
            100_000_000,
            0,
        )
        .execute_with(stats::body(CLEAN), None, &parser, || {
            (42, stats::body(CLEAN))
        });
    assert_eq!(not_ready.bypasses, 1);
    assert_eq!(not_ready.outcome, Some(42));
    assert_eq!(parser.calls.load(Ordering::Relaxed), 0);
    engine.set_ready(true);
    let budget = engine
        .begin_call(stats::context("strict")?, Operation::Write, "audit", 0, 0)
        .execute_with(stats::body(CLEAN), None, &parser, || {
            (42, stats::body(CLEAN))
        });
    assert_eq!(budget.request.fault, Some(EngineFault::ResourceExhausted));
    assert_eq!(budget.bypasses, 1);
    assert_eq!(parser.calls.load(Ordering::Relaxed), 0);
    // Fresh engine: only response failures should count toward three failures.
    let engine = fixture::engine()?;
    for _ in 0..3 {
        let output = engine
            .begin_call(
                stats::context("repair")?,
                Operation::Write,
                "audit",
                100_000_000,
                0,
            )
            .execute_with(stats::body(CLEAN), None, &parser, || {
                (42, stats::body(CLEAN))
            });
        assert_eq!(output.bypasses, 1);
        assert_eq!(output.request.invariants.len(), 6);
    }
    let output = engine
        .begin_call(
            stats::context("repair")?,
            Operation::Write,
            "audit",
            100_000_000,
            0,
        )
        .execute_with(stats::body(CLEAN), None, &parser, || {
            (42, stats::body(CLEAN))
        });
    assert_eq!(output.request.fault, Some(EngineFault::CircuitOpen));
    assert_eq!(output.outcome, Some(42));
    assert_eq!(parser.calls.load(Ordering::Relaxed), 6);
    Ok(())
}
#[test]
fn incomplete_and_duplicate_inputs_cannot_claim_invariant_evidence() -> Result<(), Error> {
    for (bytes, truncated, bypass) in [
        (CLEAN, true, false),
        (br#"{"total":1,"total":2}"#.as_slice(), false, true),
    ] {
        let mut body = stats::body(bytes);
        body.truncated = truncated;
        let output = fixture::engine()?
            .begin_call(
                stats::context("repair")?,
                Operation::Write,
                "audit",
                100_000_000,
                0,
            )
            .dispatch(body, None, || bytes, |b| stats::body(b));
        assert!(output.request.invariants.is_empty());
        assert!(output.request.invariant_audit.is_none());
        assert_eq!(output.bypasses, u8::from(bypass));
        assert!(std::ptr::eq(output.outcome.ok_or("outcome")?, bytes));
    }
    Ok(())
}
