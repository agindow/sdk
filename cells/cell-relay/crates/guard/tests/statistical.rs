#[path = "support/statistical.rs"]
mod support;
use agw_diff::Side;
use agw_guard::statistical::{Decision, Engine, Operation, Plan};
use support::{context, field, identity};

type Result = std::result::Result<(), Box<dyn std::error::Error>>;
#[test]
fn semantic_request_blocks_before_dispatch_but_response_retains_completed_outcome() -> Result {
    for side in [Side::Request, Side::Response] {
        let engine = Engine::statistical(Plan::new(identity(), vec![field(side, "1", "100")?])?);
        let call = engine.begin_call(
            context("repair")?,
            Operation::Write,
            "audit",
            100_000_000,
            0,
        );
        let count = std::cell::Cell::new(0);
        let bytes = b"{\"value\":100}";
        let output = call.execute(support::body(bytes), None, || {
            count.set(count.get() + 1);
            ("original", support::body(bytes))
        });
        assert_eq!(output.dispatched, side == Side::Response);
        assert_eq!(count.get(), usize::from(side == Side::Response));
        assert_eq!(
            output.request.decision,
            if side == Side::Request {
                Decision::Halt
            } else {
                Decision::Pass
            }
        );
        if side == Side::Response {
            assert_eq!(output.outcome, Some("original"));
            assert_eq!(
                output.response.as_ref().map(|p| p.decision),
                Some(Decision::Halt)
            );
        }
        assert_eq!(output.bypasses, 0);
    }
    Ok(())
}

#[test]
fn all_detectors_run_on_both_sides_with_real_pvalues_and_typed_provenance() -> Result {
    use agw_guard::statistical::{Detector, Evidence};
    use agw_stats::significance::{Hypothesis, Method};
    let cases = [
        ("1", "100", Detector::Unit),
        ("\"old\"", "\"new\"", Detector::Enumeration),
        ("1", "null", Detector::Completeness),
        ("1", "\"text\"", Detector::TypeConsistency),
        ("\"alpha\"", "\"12345\"", Detector::Charset),
    ];
    for side in [Side::Request, Side::Response] {
        for (baseline, live, target) in cases {
            let field = field(side, baseline, live)?;
            let bytes = format!("{{\"value\":{live}}}");
            let engine = Engine::statistical(Plan::new(identity(), vec![field])?);
            let runner = Counter::default();
            let call = engine.begin_call(
                context("observe")?,
                Operation::Write,
                "audit",
                100_000_000,
                0,
            );
            let original = Box::new(17);
            let ptr = &*original as *const i32;
            let output = call.execute_with(support::body(bytes.as_bytes()), None, &runner, || {
                (original, support::body(bytes.as_bytes()))
            });
            assert_eq!(output.bypasses, 0);
            assert_eq!(
                output.outcome.as_deref().map(|v| v as *const i32),
                Some(ptr)
            );
            assert_eq!(output.family, 535);
            let phase = if side == Side::Request {
                &output.request
            } else {
                output.response.as_ref().ok_or("response")?
            };
            assert_eq!(phase.decision, Decision::Pass);
            let record = phase
                .records
                .iter()
                .find(|r| r.detector == target)
                .ok_or("detector not invoked")?;
            assert_eq!(record.side, side);
            assert_eq!(record.path, "/value");
            assert_eq!(record.generation, "v1");
            assert!(
                record
                    .eligibility
                    .iter()
                    .any(|d| d.is_some_and(|v| v.eligible())),
                "{target:?}"
            );
            assert!(runner.total() > 0);
            assert_eq!(runner.total(), phase.records.len());
            for record in &phase.records {
                for test in record.evidence.tests() {
                    assert_eq!(test.evidence().tests, 535);
                    assert!(test.evidence().p_value > 0.0);
                    assert_eq!(
                        test.method(),
                        if *test.hypothesis() == Hypothesis::Magnitude {
                            Method::MagnitudeDkwBound
                        } else {
                            Method::FisherExact
                        }
                    );
                }
            }
            if let Evidence::Enumeration(report) = &record.evidence {
                let test = report
                    .tests()
                    .iter()
                    .find(|t| matches!(t.hypothesis(),Hypothesis::Category(value) if value=="new"))
                    .ok_or("novel-category test")?;
                assert_eq!(test.samples(), (128, 129));
                assert_eq!(
                    test.evidence().p_value,
                    agw_stats::significance::fisher(0, 128, 129, 129)?.p_value
                );
            }
            if let Evidence::Unit(report) = &record.evidence {
                let a = vec![1.0; 128];
                let b = vec![100.0; 129];
                assert_eq!(
                    report.tests()[0].evidence().p_value,
                    agw_stats::significance::magnitude(&a, &b)?.p_value
                );
                assert!(matches!(
                    report.detection(),
                    agw_stats::unit::Detection::Drift(_)
                ));
            }
        }
    }
    Ok(())
}

#[derive(Default)]
struct Counter(std::sync::atomic::AtomicUsize);
impl Counter {
    fn total(&self) -> usize {
        self.0.load(std::sync::atomic::Ordering::Relaxed)
    }
}
impl agw_guard::statistical::Detectors for Counter {
    fn run(
        &self,
        detector: agw_guard::statistical::Detector,
        path: &str,
        a: &agw_stats::windows::Window,
        b: &agw_stats::windows::Window,
        family: usize,
    ) -> std::result::Result<agw_guard::statistical::Evidence, agw_guard::availability::EngineFault>
    {
        self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        agw_guard::statistical::Detectors::run(
            &agw_guard::statistical::Builtins,
            detector,
            path,
            a,
            b,
            family,
        )
    }
}

#[test]
fn shared_breaker_opens_after_three_response_faults_and_skips_later_request_work() -> Result {
    use agw_guard::{
        availability::EngineFault,
        statistical::{Detector, Detectors, Evidence},
    };
    use agw_stats::windows::Window;
    struct Failing(std::sync::atomic::AtomicUsize);
    impl Detectors for Failing {
        fn run(
            &self,
            _: Detector,
            _: &str,
            _: &Window,
            _: &Window,
            _: usize,
        ) -> std::result::Result<Evidence, EngineFault> {
            self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Err(EngineFault::Internal)
        }
    }
    let engine = Engine::statistical(Plan::new(
        identity(),
        vec![field(Side::Response, "1", "1")?],
    )?);
    let failing = Failing(std::sync::atomic::AtomicUsize::new(0));
    for i in 0..5 {
        let output = engine
            .begin_call(
                context("strict")?,
                Operation::Write,
                "audit",
                100_000_000,
                0,
            )
            .execute_with(support::body(b"{}"), None, &failing, || {
                ("original", support::body(b"{\"value\":1}"))
            });
        assert_eq!(output.bypasses, 1);
        assert_eq!(output.outcome, Some("original"));
        if i >= 3 {
            assert_eq!(output.request.fault, Some(EngineFault::CircuitOpen));
        }
    }
    assert_eq!(failing.0.load(std::sync::atomic::Ordering::Relaxed), 3);
    Ok(())
}

#[test]
fn real_upstream_owned_response_is_inspected_by_borrow_without_cloning_or_replay() -> Result {
    let engine = Engine::statistical(Plan::new(
        identity(),
        vec![field(Side::Response, "1", "100")?],
    )?);
    let count = std::cell::Cell::new(0);
    let pointer = std::cell::Cell::new(std::ptr::null());
    let output = engine
        .begin_call(
            context("repair")?,
            Operation::Write,
            "audit",
            100_000_000,
            0,
        )
        .dispatch(
            support::body(b"{}"),
            None,
            || {
                count.set(count.get() + 1);
                let bytes = b"{\"value\":100}".to_vec();
                pointer.set(bytes.as_ptr());
                bytes
            },
            |bytes| support::body(bytes),
        );
    assert_eq!(count.get(), 1);
    assert_eq!(output.response.ok_or("response")?.decision, Decision::Halt);
    let preserved = output.outcome.ok_or("outcome")?;
    assert_eq!(preserved.as_ptr(), pointer.get());
    assert!(output.dispatched);
    Ok(())
}
