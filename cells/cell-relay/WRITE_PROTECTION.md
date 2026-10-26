# — explicit write-protection opt-out

Native controls implement [contract §4.1 and §10.36](/agindow-runtime-contract.md).
`RuntimeConfig` defaults to protected writes. Use one runtime per tenant/connection
protection scope. Administrative callers can request an opt-out:
```rust,ignore
let receipt = runtime.reload_fail_closed(false, authenticated_actor, utc_timestamp)?;
// Persist the attributed receipt and return its warning to the control caller.
let context = runtime.begin_call(None)?;
// Pass context to gated::Call or statistical::Engine::begin_call.
```
Every false request returns a warning, even in strict mode. Empty/control-character
actors and actors longer than 256 bytes are rejected before changing state.
Receipts are serializable administrative audit drafts; the library does not
publish or persist them. The authenticated host binds scope, serializes controls,
and retains the actor/timestamp receipt off-path. Restoration also returns a
receipt. There is no implicit environment-variable opt-out.

| Unresolved finding | Repair, default | Repair, opt-out | Strict, either flag |
|---|---|---|---|
| Shape write | Degraded passthrough | Degraded passthrough | Halt |
| Unknown write | Halt | Degraded passthrough | Halt |
| Semantic write | Halt | Halt | Halt |
| Read, any class | Degraded passthrough | Degraded passthrough | Halt |

Observe, verified repair, malformed-body and availability rules are unchanged.
Unknown operations receive conservative write policy. Mode and protection occupy
one atomic configuration word, loaded once per call. Changes cannot alter a
request/response pair already in progress. Call audits record the effective
`fail_closed` value in `metadata.agindow_policy`; administrative actor/time lives
in the separate control receipt. Original bytes and completed upstream outcomes
remain available through the existing guard APIs.

The consumes host-adapted accepted
receipts and cumulative detected-write counts. Deliver those off-path. Count
admitted drift on both phases once per call, including observed, repaired and
blocked calls; exclude clean calls, read calls and faults without drift evidence.
The counter's epoch is the control revision that first changed protection to
false, retained until restoration. Delivery, durable counters, authentication and
a mounted hosted dashboard are not supplied here; the default engine remains
`NullEngine`.
