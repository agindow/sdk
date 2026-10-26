# — native idempotency and replay safety

The explicit `agw_guard::idempotency` boundary implements
[runtime contract §5 and §10.37](/agindow-runtime-contract.md). Keep one
`Store` per authenticated tenant and reuse it across that tenant's calls. Its
key is exactly `(connection, idempotency_key)`, case-sensitive. Separate stores,
processes or hosts do not share reservations.
```rust,ignore
use agw_guard::idempotency::{Access, Bindings, Limits, Request, Store};

// Set bindings from the endpoint's known capabilities; no provider is guessed.
let store = Store::new(tenant, Limits::default(),
    Bindings::new("Idempotency-Key", supported_durability_header)?)?;
let result = store.execute(Request {
    connection, operation, key: Some(idempotency_key), durability: requested_hint,
}, |forwarding| {
    // Apply on every host-supplied attempt, including an explicitly repaired one.
    forwarding.apply_headers(&mut upstream_headers);
    send_owned_response(upstream_headers)
})?;
match result {
    Access::Completed(replay) => deliver_original(replay.value),
    Access::InProgress => attach_to_current_call(),
    Access::Indeterminate => reconcile_the_existing_write(),
}
```
Writes and `unknown` operations require a key in every mode, even while the
engine is warming. Missing/invalid keys return `invalid_request` (400) before
evaluation or dispatch. Keys are 1–256 visible ASCII bytes, without whitespace
or control characters. Connection/tenant identifiers are nonempty, at most 256
bytes, and contain no control characters. Reads do not consume or wait on write
reservations. Supplied read metadata must still be valid.

`Bindings` validates HTTP token names and refuses conflicting key/durability
header names. `Forwarding::apply_headers` replaces all case-insensitive copies
of the claimed header, appending one exact supplied value. Other headers and
customer body bytes are untouched. Reuse the immutable forwarding view on every
explicit transport attempt. No retry, repair proposal or backoff is generated
by the store; retry ceilings remain deferred in contract §10.5.

## Original outcomes and concurrency

`Access::Completed` holds an `Arc<T>`, without requiring `T: Clone`. All replays
share the original object, including its upstream error identity. Hosts use
bounded, immutable owned terminal response values. A shared consuming stream
reader is not replayable response storage. The store bounds metadata and entry
count, not arbitrary caller-owned heaps.

A key reserves execution before invoking any callback. Concurrent identical keys
wait on that entry and return its original outcome. The global map lock is never
held over caller work, upstream I/O, waiting or disposal of expired outcomes.
Different keys proceed independently; `Store::waiting` exposes a diagnostic
waiter count, never an authorization check. Same-thread recursive access to the
running key returns `InProgress` instead of deadlocking. Waiting has no forced
execution timeout: abandoning a slow reservation must not issue another write.
The host owns request cancellation/concurrency and upstream timeouts.

All completed outcomes are terminal during their window: successes, upstream
errors, policy blocks, and inspection bypasses. Different arguments, modes,
durability hints or contracts under the same key return that first outcome and
its original durability applicability. Replays skip detection, verification and
dispatch. Blocked keys cannot be used to reevaluate corrected arguments. After
confirming the original request was not dispatched, an authorized corrected call
is a new logical write with a new key, correlated to the original block for
recovery measurement. Never rotate keys automatically for uncertain writes.

A caller panic propagates with its original payload. Reservation cleanup wakes
followers and retains `Indeterminate`, because a write may have completed before
the unwind. It never fabricates an outcome or dispatches again. Reconcile using
the same upstream key. `InProgress` and `Indeterminate` are coordination
dispositions, not 5xx instructions to bypass and issue another write. Hosting
adapters must preserve non-execution while reconciling. Native aborts/process loss
require upstream coordination.

## Retention, capacity and durability

Defaults are 1,024 entries and 24 hours; configure `Limits` at construction.
Capacity is 1–10,000 entries and retention is nonzero through 7 days. The window
starts when completion/indeterminacy is published. Running writes never expire.
Expired records are reclaimed on later write admission. A full unexpired cache
returns configured `rate_limited` admission (429), retaining all prior terminal
and running keys. Reads remain independent. Inspection faults bypass inside an
admitted call and their original outcomes are cached normally.

**Expiry permits another execution with the same key and is a duplicate-write
risk.** Restart, independent stores and multiple hosts also lose local coverage.
Configure retention with the upstream's actual deduplication behavior and use
upstream/durable coordination where needed. This library supplies process-local
replay safety; it cannot prove global exactly-once execution.

`durability` is an opaque 1–128 visible-ASCII-byte hint. An explicitly configured
supported binding forwards it verbatim. Without that binding, the result reports
`DurabilityStatus::Unsupported` and exposes a warning; the hint is not forwarded.
`Forwarded` identifies the supported parameter supplied to the transport callback.
The host must apply that view to its real transport (or pass its fields to the
upstream SDK); it does not attest transport delivery or verified persistence.
No provider-specific semantics or support are inferred.

## integration

`gated::Call::execute_once(&store, metadata, body, pipeline, audit, operation)`
wraps the existing bounded-capture guard. The operation callback receives effective
request bytes and the immutable forwarding view. The draft operation must match
the request envelope; mismatch returns 400 before execution.

The returned `Completion<O>` owns the original upstream `O`, audit, headers,
feedback, phase decisions, bypass count and optional repaired response bytes.
Only repair-created response bytes need separate storage; unmodified inspection
captures are never copied into the cache. Host-owned `O` must retain the complete
replayable upstream outcome. Deliver the response patch when present and honor
policy halts before exposing response bytes. A response halt retains the already
completed upstream outcome for reconciliation and replays the same terminal halt.

Audit proof records the verbatim key and leaves `write_integrity: false`.
`metadata.agindow_idempotency` records tenant, connection, durability applicability
and process-local retention. Payload verification, replay protection and forwarding
a durability hint do not prove write correctness. The same audit ID and structured
feedback survive replay; feedback explains that the key is terminal. Audit
publication/retrieval remains.

No hosted/SDK transport, automatic retries, default-engine switch, durable cache,
statistical final-audit composition, streaming response cache, production
qualification or provider persistence guarantee is added.
```sh
cargo test -p agw-guard --test idempotency --test idempotency_concurrency --test idempotency_gated
```