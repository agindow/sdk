# Explicit native two-layer calls

`agw_guard::gated::Call` connects the existing availability gate, policy matrix
and selective repair library for bounded JSON captures. It is an opt-in library
seam; the default statistical engine remains `NullEngine` and streaming/SDK
integration is separate. See runtime contract
the runtime contract.

The host prepares a local `Pipeline`, one shared `AvailabilityGate`, and a valid
generated `AuditArtifact` draft carrying identity, operation and selected contract
provenance. It snapshots `RuntimeConfig::begin_call` once, then constructs
`Call::new(context, &gate, work_limit)` and calls `execute` with the request
capture, pipeline, draft and upstream closure. Unknown operations follow writes.

`Pipeline::detect` supplies a complete side-matched report and bounded
 proposals. `Pipeline::verify` independently checks the patched payload and
the supplied applied finding indices against authoritative evidence. A callback
returning false leaves the original bytes subject to policy; an error or unwind
bypasses. No default verifier, remote callback, registry fetch or LLM call exists.
Callbacks must charge the shared cooperative budget before work/allocation.
There is no hard preemption or process-wide allocation-failure recovery.

The upstream closure receives the effective request bytes and runs once unless
request policy halts. It returns the untouched upstream outcome and a borrowed
response capture. The result retains that outcome by ownership/reference and
separately exposes effective response bytes plus disposition. This convenience
API requires response captures to outlive the call result; it does not buffer
streams or create self-referential borrows into the owned outcome. Hosts honor
response halts before delivering bytes. A response halt reports a completed
attempt and cannot undo a write.

Observe records drift but never verifies, applies or blocks. Other modes use
`agw-policy` per unresolved finding class, with semantic precedence. A verified
patch is the only path to owned effective bytes. A request blocked by another
unresolved finding sends nothing and claims no applied patch. Incomplete,
duplicate-key, skipped, invalid-side and over-budget evidence bypasses instead
of becoming vendor drift. Complete malformed bodies retain 's special
shape policy and byte-identity audit evidence.

One readiness admission, circuit permit, mode snapshot and work allowance span
both phases. Request faults dispatch original bytes and skip response work.
Response faults return original response bytes and retain a verified request
patch already sent. Availability overrides the final policy disposition and is
counted once, independently of malformed-body policy's `bypassed` wire status.

The result includes claimed AGINDOW headers, a warning and one owned audit v1
draft. The host merges these headers with upstream headers using `Headers::claim`
and publishes the warning/artifact asynchronously. Mode, decision and reason
are aligned; `metadata.agindow_policy` is reserved for per-phase reason/fault/
decision, dispatch and bypass accounting. Reused draft drift/patch/confidence/
hypothesis/note claims are replaced. Applied patch paths carry `request:` or
`response:` prefixes, while `X-Agindow-Patch` gives a compact selective field
count. Payload verification does not establish write integrity or idempotency:
those stale proof claims are cleared. Contract version and verification source
remain host-supplied provenance. owns statistical evidence composition;
/ own structured errors and storage/publication.

Validation:

```sh
cargo test -p agw-guard --test gated --test gated_edges --test gated_detector
cargo test -p agw-policy
```

The detector fixture runs actual identity-backed comparison and independent
target-schema verification over 's token patcher, retaining `1e400` verbatim.
Other fixtures cover modes/operations/classes/repair outcomes, faults in each
phase, cold/open gates, verifier faults, malformed/incomplete captures, reloads,
byte identity and completed upstream errors. These are local correctness tests,
not production repair calibration or new native/WASM latency claims.

## Explicit replay safety

 adds `Call::execute_once` as an opt-in boundary around this call path.
It validates write keys before inspection, serializes identical keys and retains
the original terminal result, including blocks, bypasses and completed response
halts. It caches final audit/feedback and moves repaired response bytes without
copying unmodified inspection captures. See [idempotency and retention
semantics](./IDEMPOTENCY.md); low-level `execute` does not perform local deduplication.
