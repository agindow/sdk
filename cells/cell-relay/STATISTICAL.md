# Explicit native statistical guard
`agw_guard::statistical::Engine::default` is statistical with an empty plan. Configure windows to detect. Explicitly opt into
`Engine::statistical(Plan::new(identity, fields)?)` using local snapshots.
Nothing fetches profiles, calls a provider, or activates detection by default.
Select the exact tenant, connection, vendor, endpoint, side, field, segment,
window and generation outside the request path. `Field::enumeration` declares
categorical meaning; do not enable enum novelty on arbitrary free text or IDs.

A plan holds at most seven field/comparisons across both sides. Its conservative
family reserves 535 slots per comparison: 1 magnitude + 512 enum categories +
5 completeness + 6 type + 11 charset/format features. All slots, including unused
and non-finding slots, remain in the Bonferroni denominator. There is no
production independence, repeated-peeking or calibration guarantee. Seasonal
selection is explicit host input under runtime contract §10.23; no fallback or
pooling occurs in this guard.

## Executing a call

Call `engine.begin_call(context, operation, audit_ref, work_units, inference_cap)`
once. The returned call owns its immutable plan selection and mode, shared
non-cloneable work/investigation allowances, and one breaker admission. Updating
`Engine::replace` or the runtime mode affects later calls only. `audit_ref` is
qualified with the plan's tenant before entering 's optional local outbox.

Use `call.dispatch(request, outbox, upstream, capture)` for owned upstream results.
`upstream` runs exactly once outside native recovery unless request policy halts.
`capture` borrows a bounded response capture from that original result and returns
`Body::complete(bytes, timestamp)` or a body with `truncated: true`. The host's
upstream and capture projection are caller code; their panics propagate. Capture
projection does not perform engine work or collect/await an unbounded stream.
For pre-existing borrowed captures, `execute` accepts a closure returning
`(original_outcome, response_body)`.

Bodies retain borrowed bytes. Parsing is lossless and never serializes a result
back onto the wire. Every field receives one transient current-call opportunity
in addition to its selected live window; at 4096 samples the oldest live sample
is omitted. Neither the live store nor its frozen baseline is updated. Body
timestamps come from the host. High-precision/unsafe projection disables unit
analysis only; unit diagnoses never create repair candidates or conversions.

The `Body::findings` input can borrow a complete local shape report. Optional
plan shape snapshots also run 's inspection comparator. They preserve its
uncertainty: learned/observed structure does not become authoritative shape proof.
The engine retains all mixed findings and typed statistical reports, methods,
sample counts, raw p-values, hypotheses, generations and suppression decisions.
Routes the combined findings; the shared `agw_policy` matrix owns policy.
Complete malformed bodies reuse 's disposition. Truncated, duplicate-key,
resource-limited and parser-faulted bodies cannot supply statistical drift.

`Outcome::dispatched` is false only for a pre-send halt. A response-side halt has
`dispatched: true` and retains the completed original outcome, including its
original vendor error. It does not claim prevention, rollback or integrity proof.
`Outcome::bypasses` counts availability failure once per call; unavailable evidence
is not a clean verdict. A failed request evaluation skips response evaluation.
Successful request work does not reset consecutive response failures in the
shared breaker. Three failed calls open its one-second monotonic cooldown.

The policy prerequisite is 's pure matrix in `agw-policy`. This ticket adds
that boundary and its exhaustive matrix test; 's selective repair integration,
HTTP/SDK and streaming drivers remain separate. Composes this call's
statistical evidence into one audit artifact (below). Strict reads halt on
unverified drift as specified by contract §2; repair reads fail open. Observe
preserves original bytes and outcomes in every case.

Investigation uses only 's bounded outbox. One shared call reservation admits
at most one job across both phases. Missing/disabled/full queues, zero ceilings
and exhausted inference caps cannot alter policy. Jobs contain tenant-qualified
audit references, not payloads. No worker, network, persistence or inference runs.

## Composing one audit artifact

`outcome.compose(draft)` returns the one final `audit_artifact` v1 value; use
`compose_with(draft, &applied)` to report caller-verified patches. Composition
preserves the decision and frozen mode, replaces the reserved metadata keys
(`agindow_statistical`, `agindow_policy`, `agindow_invariants`,
`agindow_cross_endpoint`, `agindow_activation`, `agindow_enum_growth`), and clears
stale `patch`, `hypothesis`, `confidence`, `note`, the idempotency key and any
write-integrity claim from the draft. A supplied `Applied` entry is reported only
when `verified` is true and it names fields; it still never claims write integrity.

Evidence lives under `metadata.agindow_statistical` (`agindow_statistical/1`):
identity, per-field generation/window/segment/readiness/sample provenance,
per-phase evaluation and suppression counters, the final policy projection, one
typed entry per admitted test, and the fixed limits/truncation record. Unit
`unit_conversion` hypotheses remain diagnoses for and never authorize
conversion. Enum `window_first_seen` is window-only; recurrence belongs to.
A finding carries `side`, `path`, `detector`, `class`, `signal`, sample/p-value
counts, `reason` and a multi-entry `hypotheses` array; the artifact-level
`hypothesis` is set only when exactly one hypothesis kind exists.

Composition is deterministic under input permutation and bounded at 256 findings,
1,024 changes and 256 KiB of serialized findings. Omitted counts/bytes are
reported in `truncated`; provenance, evaluation counters, decision and mode are
never truncated, and the aggregate class is a maximum (shape < unknown <
semantic). `outcome.publish(draft, &mut publisher, now)` forwards the composed
artifact through 's local publisher and returns `None` on local store or
honesty refusal. A full outbox still returns a publication with `queued: false`;
it never awaits a sink, replaces the upstream outcome or changes policy.
The shared fixture `contracts/examples/audit_artifact.statistical.valid.json`
round-trips through the frozen schema in Rust and TypeScript. See runtime contract
§10.41 and [AUDIT.md](./AUDIT.md).

Run:
```sh
cargo test -p agw-guard --test statistical_audit --test statistical_audit_limits
npx vitest run tests/unit/contracts.test.ts
```
## Resource and latency validation

The cooperative work allowance precharges parsing/extraction and result copying,
all detector features/test supports, routing and window costs. Per phase it charges
16 Mi work units + 256 × captured bytes +, per selected ready field,
64 × accounted snapshot bytes + 32 × 535 × (baseline + live + 1 samples).
Call-reference construction charges 4096 + reference bytes once. Work units are
conservative accounting, not elapsed time or measured RSS. Exhaustion bypasses;
process OOM/abort and preemption remain outside native unwind containment.

Run:
```sh
cargo bench -p agw-guard --bench statistical
node cells/cell-relay/perf/run-availability.mjs
```
The integrated native Criterion fixture includes call entry, lossless parsing,
transient extraction, applicable adapters, materiality, routing,
Policy, containment, and evidence destruction on both sides. Baseline setup,
network/streaming, capture acquisition, SDK/WASM and final audit composition are
excluded. Fixtures use 128 samples per window, observe mode, 100 measurements,
1 s warmup and 3 s sampling. Results are local measurements, not an integrated
latency gate or a production SLA. Separately gates native bypass means.

Local run on October 17, 2026: Apple M2, Darwin arm64, Rust 1.99.1, release
profile. Arithmetic iteration means from Criterion's `new/estimates.json`:

| Native case | Mean (µs) | 95% mean interval (µs) |
|---|---:|---:|
| Numeric fields, both phases | 123.5998 | 122.0956–125.3190 |
| Enum + string fields, both phases | 42.7193 | 42.5389–42.9077 |
| 862-byte fixture, both phases | 147.2043 | 146.0393–148.5635 |
| Configured not-ready bypass | 0.0196 | 0.0194–0.0199 |

These are fixture means, not p50/p99 validation or a portable latency guarantee.
Criterion reports remain under `target/criterion/statistical/` (not committed).

See for acceptance criteria,
For policy ownership and the
[runtime contract](/agindow-runtime-contract.md) §10.27 for the decisions.

Optionally attaches frozen request/response invariant selections to this
same plan and call-level containment. See [invariant execution](./INVARIANTS.md)
for exact projection, enum additions, shared policy, per-phase audit drafts and
the local integrated native p99 gate. Empty statistical fields are supported.

Retains `Engine::NullEngine` only as an explicit native opt-out/test seam.
No Node/Python/native constructor selects it implicitly. An unselected statistical
plan performs no detector or capture work; readiness faults still bypass.
