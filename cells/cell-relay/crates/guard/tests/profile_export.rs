//! TI-48 handoff cannot acquire a sender during synchronous guard evaluation.
#[path = "support/statistical.rs"]
mod support;
use agw_diff::Side;
use agw_guard::statistical::{Engine, Operation, Plan};
use agw_inspector::{Precision, inspect};
use agw_stats::{
    export::{StageLimits, Staging},
    windows::{Limits, Projection, Selection, Store},
};

#[test]
fn staging_pressure_preserves_upstream_bytes_and_outcome_identity_without_remote_work()
-> Result<(), Box<dyn std::error::Error>> {
    for cold in [false, true] {
        let field = support::field(Side::Response, "1", "1")?;
        let selection = Selection {
            scope: field.scope.clone(),
            projection: Projection::ExactInteger(Precision::Observed),
        };
        let engine = Engine::statistical(Plan::new(support::identity(), vec![field])?);
        engine.set_ready(!cold);
        let mut store = Store::new(Limits {
            samples: 16,
            scopes: 1,
            bytes: 100_000,
            batch: 1,
            inspection_nodes: 100,
            inspection_bytes: 1000,
            minimum: 1,
        })?;
        let mut staging = Staging::new(StageLimits {
            windows: 1,
            bytes: 100_000,
        })?;
        let bytes = b"{ \"value\" : 1 }";
        let mut sends = 0;
        for call_id in ["one", "two"] {
            let original = Box::new(bytes.to_vec());
            let ptr = original.as_ptr();
            let call = engine.begin_call(
                support::context("observe")?,
                Operation::Write,
                "audit",
                100_000_000,
                0,
            );
            let at = support::body(bytes).at;
            let outcome = call.dispatch(
                support::body(bytes),
                None,
                || {
                    let body = original.as_slice();
                    // Application-owned local collector; errors never replace outcome.
                    let _ = store.record(
                        std::slice::from_ref(&selection),
                        &inspect(body, false, None),
                        at,
                    );
                    let _ = staging.close(
                        &mut store,
                        &selection.scope,
                        call_id,
                        at,
                        at + chrono::Duration::seconds(1),
                    );
                    original
                },
                |body| agw_guard::statistical::Body::complete(body, at),
            );
            let retained = outcome.outcome.ok_or("upstream")?;
            assert_eq!(retained.as_slice(), bytes);
            assert_eq!(retained.as_ptr(), ptr);
            assert_eq!(sends, 0);
            assert_eq!(outcome.bypasses, u8::from(cold));
        }
        assert_eq!(staging.len(), 1);
        assert_eq!(staging.rejected(), 1);
        // Only an explicit background owner can pop/project/send the handoff.
        while staging.pop().is_some() {
            sends += 1;
        }
        assert_eq!(sends, 1);
    }
    Ok(())
}
