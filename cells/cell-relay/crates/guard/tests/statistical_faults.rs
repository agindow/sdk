#[path = "support/statistical.rs"]
mod support;
use agw_diff::Side;
use agw_guard::{
    availability::EngineFault,
    statistical::{Builtins, Decision, Detector, Detectors, Engine, Evidence, Operation, Plan},
};
use agw_stats::windows::{Availability, Window};
use std::sync::atomic::{AtomicUsize, Ordering};
use support::{body, context, field, identity};
type Result = std::result::Result<(), Box<dyn std::error::Error>>;
struct Runner {
    calls: AtomicUsize,
    fault: Option<EngineFault>,
    panic: bool,
    parser: bool,
}
impl Runner {
    fn new(fault: Option<EngineFault>, panic: bool, parser: bool) -> Self {
        Self {
            calls: AtomicUsize::new(0),
            fault,
            panic,
            parser,
        }
    }
}
impl Detectors for Runner {
    fn inspect(&self, bytes: &[u8]) -> agw_inspector::Inspection {
        if self.parser {
            if self.panic {
                panic!("parser unwind")
            }
            return agw_inspector::Inspection {
                truncated: false,
                parse: agw_inspector::Parse::Unparseable(agw_inspector::ParseFailure {
                    class: agw_inspector::ParseClass::Data,
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
        d: Detector,
        path: &str,
        a: &Window,
        b: &Window,
        family: usize,
    ) -> std::result::Result<Evidence, EngineFault> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        if self.panic {
            panic!("detector unwind")
        }
        if let Some(fault) = self.fault {
            return Err(fault);
        }
        Builtins.run(d, path, a, b, family)
    }
}
#[test]
fn native_faults_bypass_once_preserve_original_error_identity_and_skip_second_phase() -> Result {
    for mode in ["observe", "repair", "strict"] {
        for (fault, panic, parser) in [
            (Some(EngineFault::Internal), false, false),
            (None, true, false),
            (None, true, true),
            (None, false, true),
        ] {
            let engine = Engine::statistical(Plan::new(
                identity(),
                vec![
                    field(Side::Request, "1", "100")?,
                    field(Side::Response, "1", "100")?,
                ],
            )?);
            let runner = Runner::new(fault, panic, parser);
            let count = std::cell::Cell::new(0);
            let error = Box::new("vendor error");
            let ptr = &*error as *const &str;
            let output = engine
                .begin_call(context(mode)?, Operation::Write, "audit", 100_000_000, 0)
                .execute_with(body(b"{\"value\":100}"), None, &runner, || {
                    count.set(count.get() + 1);
                    (Err::<(), _>(error), body(b"{\"value\":100}"))
                });
            assert_eq!(output.bypasses, 1);
            assert!(output.response.is_none());
            assert_eq!(count.get(), 1);
            let preserved = output
                .outcome
                .ok_or("outcome")?
                .err()
                .ok_or("vendor error")?;
            assert_eq!(&*preserved as *const &str, ptr);
            assert_eq!(runner.calls.load(Ordering::Relaxed), usize::from(!parser));
            assert!(output.request.report.changes.is_empty());
            assert_eq!(output.request.decision, Decision::Pass);
        }
    }
    Ok(())
}
#[test]
fn cold_missing_corrupt_not_ready_and_budget_exhaustion_skip_work_and_do_not_report_clean() -> Result
{
    for state in 0..6 {
        let mut selection = field(Side::Request, "1", "100")?;
        selection.snapshot = match state {
            0 => Availability::Warming {
                baseline: 0,
                live: 0,
            },
            1 => Availability::Unavailable(agw_stats::windows::Error::UnknownScope),
            2 => Availability::Unavailable(agw_stats::windows::Error::InvalidInput),
            _ => selection.snapshot,
        };
        let engine = Engine::statistical(Plan::new(identity(), vec![selection])?);
        if state == 3 {
            engine.set_ready(false)
        }
        let runner = Runner::new(None, false, false);
        let output = engine
            .begin_call(
                context("strict")?,
                Operation::Write,
                "audit",
                if state >= 4 { 0 } else { 100_000_000 },
                0,
            )
            .execute_with(body(b"{\"value\":100}"), None, &runner, || {
                (42, body(b"{}"))
            });
        assert_eq!(runner.calls.load(Ordering::Relaxed), 0);
        assert_eq!(output.bypasses, 1);
        assert_eq!(output.outcome, Some(42));
        assert!(output.request.fault.is_some());
        assert!(output.request.records.is_empty());
    }
    Ok(())
}
#[test]
fn truncated_malformed_duplicate_and_parser_fault_inputs_cannot_generate_statistics() -> Result {
    for mode in ["observe", "repair", "strict"] {
        for (bytes, truncated, malformed) in [
            (b"{}".as_slice(), true, false),
            (b"{", false, true),
            (b"{\"x\":1,\"x\":2}", false, false),
        ] {
            let engine = Engine::statistical(Plan::new(
                identity(),
                vec![field(Side::Request, "1", "100")?],
            )?);
            let runner = Runner::new(None, false, false);
            let mut request = body(bytes);
            request.truncated = truncated;
            let output = engine
                .begin_call(context(mode)?, Operation::Write, "audit", 100_000_000, 0)
                .execute_with(request, None, &runner, || (bytes, body(b"{}")));
            assert_eq!(runner.calls.load(Ordering::Relaxed), 0);
            assert!(output.request.records.is_empty());
            assert_eq!(output.request.malformed.is_some(), malformed);
            if truncated {
                assert_eq!(
                    output.request.report.skipped,
                    Some(agw_diff::Skip::Incomplete)
                );
                assert!(output.request.report.changes.is_empty());
            }
            if malformed && mode == "strict" {
                assert!(!output.dispatched)
            } else {
                assert!(std::ptr::eq(output.outcome.ok_or("outcome")?, bytes));
            }
        }
    }
    Ok(())
}
#[test]
fn null_engine_has_zero_friction_even_without_a_budget_or_valid_audit_reference() -> Result {
    let engine = Engine::default();
    let runner = Runner::new(None, true, true);
    let output = engine
        .begin_call(context("strict")?, Operation::Write, "", 0, 0)
        .execute_with(body(b"{"), None, &runner, || ("untouched", body(b"{")));
    assert_eq!(output.outcome, Some("untouched"));
    assert_eq!(output.bypasses, 0);
    assert_eq!(runner.calls.load(Ordering::Relaxed), 0);
    Ok(())
}
#[test]
fn upstream_panic_is_never_converted_to_an_engine_fault() -> Result {
    let engine = Engine::statistical(Plan::new(identity(), vec![])?);
    let panic = std::panic::catch_unwind(|| {
        engine
            .begin_call(
                context("repair").unwrap_or_else(|e| panic!("{e}")),
                Operation::Read,
                "audit",
                100_000_000,
                0,
            )
            .execute::<(), _>(body(b"{}"), None, || std::panic::panic_any(73u32))
    });
    assert_eq!(
        panic
            .err()
            .and_then(|p| p.downcast::<u32>().ok())
            .as_deref(),
        Some(&73)
    );
    Ok(())
}

#[test]
fn default_engine_does_not_request_a_response_capture() -> Result {
    let output = Engine::default()
        .begin_call(context("repair")?, Operation::Read, "", 0, 0)
        .dispatch(
            body(b"{}"),
            None,
            || "original",
            |_| panic!("disabled engine must not collect a capture"),
        );
    assert_eq!(output.outcome, Some("original"));
    Ok(())
}
