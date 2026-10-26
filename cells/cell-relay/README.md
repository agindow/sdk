# cell-relay — the in-path engine

**Rust** · Apache-2.0 (the tech stack) · **Byte-passthrough forwarding + bounded inspection tee + mode configuration + schema learning + lossless inspection + generated contract types — explicit native guard paths; `NullEngine` remains the default**

> Sits in the request path of every tool call: intercepts, inspects on a side
> channel, classifies drift, repairs selectively, records the artifact. Deploys
> as a native binary and a WASM embed from one core.

## The rules this cell cannot break

| Rule | Where |
|---|---|
| No synchronous dependency on another cell, a database, or an LLM | the tech stack |
| Never reserialize the client's payload — byte-identity is structural | contract §3, the agent guide |
| Reads never block; shape drift fails open, semantic drift fails closed | contract §4.2 |
| Any internal fault becomes `bypassed`, never a thrown error | contract §4.1 |
| Availability-gate path < 10 µs, faster than the working path | the tech stack |

These are why the cell owns its own pipeline: a broken neighbour must not be able
to stop it, and it must not be able to stop a neighbour.

## Crates

Directories follow the tech stack; package names carry the `agw-` prefix.

| Crate | Responsibility | Lands with |
|---|---|---|
| `agw-node-guest` | embedded Node WASM binding and local statistical selection | |
| `agw-guard` | interception, byte-passthrough forwarding, mode dispatch | |
| `agw-inspector` | streaming tee, schema inference |, |
| `agw-diff` | shape diff, classification | |
| `agw-stats` | Tier 1 statistical detectors | |
| `agw-invariants` | cross-field invariant checks | |
| `agw-repair` | selective patch synthesis | |
| `agw-policy` | fail-open/close gates, availability gate |, |
| `agw-audit` | audit artifact construction; generated cross-cell contract types | |
| `agw-notify` | channel-neutral notification envelopes and bounded off-path emission | |

`agw-guard` and `agw-inspector` carry the engine behaviour that exists:
`proxy::forward` streams an upstream body to the client byte-for-byte,
proven by a property test and a 50MB stream, and `Tee` captures at most 8MiB of
that stream for side-channel inspection, recording parse failures as
classification input rather than raising them. `agw-diff` now provides the
standalone differ described below. `agw-invariants` now provides the DSL/local store and explicit native execution
([usage and limits](./INVARIANTS.md)); `agw-policy` supplies the shared / policy matrix; `agw-stats` supplies standalone unit and enum evidence; `agw-repair` has the numeric coercion fence and
Selective repair library, and
`agw-audit` carries generated contract types with the tests that
hold them to `contracts/examples/` — types, not engine behaviour. `agw-notify`
builds the frozen `notification` v1 envelope from an eligible alert and
holds a volatile 1–4,096-entry handoff that a host drains only after its sink
accepts; it performs no I/O and claims no durable admission. Where there
is no failing test, there is no implementation (the agent guide).

## Replay safety

`agw_guard::idempotency` supplies a bounded tenant-local replay store and explicit
the `gated::Call::execute_once` integration. Write/unknown keys are required
before inspection; replay shares original outcomes, blocks and audit identity.
Same-key calls serialize, running reservations never expire, and caller panics
retain an indeterminate result rather than causing a second execution. Key and
supported durability headers are applied verbatim. See [usage, retention risks
and host responsibilities](./IDEMPOTENCY.md). No automatic retry, durable cache,
streaming replay or default-engine change is supplied.

## Malformed-body handling

`CallContext::malformed` accepts borrowed captured bytes and baseline
inputs, returning original bytes, a disposition, headers/warning and a generated
audit artifact. Complete malformed JSON (or an authoritative top-level container
mismatch) has shape drift, confidence zero and repair skipped. Observe/repair
reads degrade; writes bypass. Strict returns `Halt` for human escalation.
Incomplete captures have no drift finding; parser faults bypass in every mode.
Caller-owned `History` accumulates scoped occurrences and first-seen timestamps
with one active entry. See contract §10.17 for scope and draft semantics.

This is a library seam. Callers must publish returned artifacts/warnings and
honor `Halt` before dispatch. Forwarding remains streaming, `NullEngine` remains
the default, and persistence/general policy wiring remain future work.

## Runtime configuration

`agw_guard::config` provides startup validation, local mode hot-reload and
immutable call snapshots. Startup precedence is explicit config > `AGINDOW_MODE`
> `repair`; a per-request override wins for that call only. Both supplied startup
sources must be valid even if one is shadowed. Values are exact lowercase
`observe`, `repair`, `strict`; invalid inputs return errors before execution.
```rust,no_run
use agw_guard::config::{Config, RuntimeConfig};

let runtime = RuntimeConfig::from_env(Config::default())?;
let call = runtime.begin_call(None)?;
// A local control-plane owner can update all clones without restart.
runtime.reload_mode("strict")?;
// `call` retains its original mode; the next begin_call sees strict.
# Ok::<(), Box<dyn std::error::Error>>(())
```
Use the same `CallContext` throughout the call: `mode` feeds future policy
logic, `forward(upstream, overhead)` claims `X-Agindow-Mode` and wraps the body
in the bounded tee, and `audit(draft)` finalizes the mode on an owned generated
`AuditArtifact` for every outcome. It leaves the draft's other fields untouched;
proof generation and storage are still future work. Forwarding remains lazy
and preserves the upstream status, unrelated headers and body bytes.

`Config.inspection_cap` accepts 0 through the inspector's 8 MiB `DEFAULT_CAP`;
0 disables capture. Caps are startup-only. Environment is read at startup only;
control-plane code calls `reload_mode` to publish validated changes. Invalid
reloads preserve the previous mode. No request reads files or environment.

These are library APIs: there is no configuration watcher, running relay
server, drift detection or mode-dependent repair/block decision yet. The
low-level `proxy::forward` retains its latency-only header contract; configured
calls use `CallContext::forward`.

## Fault containment

`CallContext::guard` evaluates a synchronous, unwind-safe engine closure with
borrowed input, a local `RegistrySnapshot`, and a per-call `Budget`. It runs the
original operation exactly once outside recovery, preserving its value or error
by identity. Internal errors, corrupt/unavailable snapshots, budget exhaustion,
and unwindable engine panics yield `Status::Bypassed(reason)` in every mode.
`Status::Inspected` means inspection completed, not that drift was classified.

Registry loading occurs outside this seam. Charge the cooperative budget before
allocating or inspecting; it cannot preempt arbitrary work or recover from actual
host OOM. It does not change the tee capture cap. The native dev/release profiles
use unwind recovery; aborts and double panics during unwinding cannot be caught.
The low-level containment boundary does not own circuit state; adds the
readiness/cooldown wrapper below. Registry loading and policy remain future work.

`wasm/guard.ts` supplies a synchronous host boundary: inspection exceptions/traps
retire that guest boundary and subsequent operations bypass without re-entry.
Create a replacement outside the request path. The host test compiles an actual
panicking Rust guest and invokes it in Node's WASM runtime; production SDK
bindings are still future work. The stable aborting WASM ABI uses host trap
recovery rather than native `catch_unwind`.

Install the test target once: `rustup target add wasm32-unknown-unknown`.
The cell's test and lint scripts include the WASM host tests and strict TypeScript
checking; its CI installs the target. No generated WASM binary is checked in.

## Availability latency gate

Attach one `agw_guard::readiness::AvailabilityGate` to an engine and share its
clones with callers of `CallContext::guard_with`. `set_ready(false)` skips
inspection. Three consecutive evaluation faults open a one-second monotonic
cooldown; one caller probes after expiry while others bypass. A failed probe
renews cooldown and a successful probe restores inspection. Successful normal
evaluations reset failures, while original-operation errors never count as engine
failures. This adds no synchronous dependencies or lock waits.

Skipped inspection closures are cleaned up under recovery before the original
operation runs. Cleanup panics yield `Bypassed(Panic)` without changing
readiness or circuit state; successful cleanup retains the admission reason.

Run the same absolute performance check as CI from the repo root:
```bash
node cells/cell-relay/perf/run-availability.mjs
```
The runner clears this benchmark's stale reports, runs the release Criterion
`availability` bench, then checks each bypass case's mean 95% confidence upper
bound is **<10,000 ns** and below the normal inspection mean's lower bound.
Missing/invalid reports fail. CI runs this outside Turbo caching and uploads
`target/criterion/availability/`, including `environment.json`, even on failure.

| Case | Timed work |
|---|---|
| `not_ready` | Readiness check; no inspection |
| `open_breaker` | Cooldown check after three actual engine failures; no inspection |
| `fresh_error` | Ready/closed guard, immediate internal error, containment and failure accounting |
| `normal_inspection` | Same ready/closed guard, budget charge, JSON parse of the 862-byte typical fixture |

All cases include an identity-return original operation, black-boxing and branch
validation. Gate creation, configuration and circuit priming are untimed using
Criterion batched setup. Each fresh-error iteration starts closed, so the case
cannot silently turn into an open-breaker benchmark. Branch validation also
rejects cooldown expiry during an open-breaker sample. There is no upstream I/O,
forwarding or production drift classification. These are native measurements;
WASM/SDK overhead and the normal path's separate p50 target remain unvalidated.

Method: 100 samples per case, 1 s warm-up, 3 s measurement, 95% confidence,
10,000 resamples. Hardware, OS, CPU count, memory and Rust/Node versions are
recorded for each run. Shared CI runner noise can fail this conservative ceiling;
inspect its uploaded report rather than silently increasing the budget.

Local validation on 2026-10-17: Apple M2 (8 logical CPUs, 8 GiB), Darwin 25.5.0
arm64, Rust 1.99.1, Node 22.21.1, release/bench profile. Bypass mean upper bounds:
not-ready 4.40 ns, open-breaker 26.56 ns, fresh-error 7.06 ns; normal inspection
mean lower bound 3,416.82 ns. These are observations on this machine, not a
production SLA. To deliberately fail the same measured reports with a 1 ns limit:
```bash
node cells/cell-relay/perf/check-availability.mjs target/criterion/availability 1
```
## Run
```bash
cargo build --workspace            # from the repo root, clean clone
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace             # guard forwarding + inspector tee + audit contract tests
```
Cell-scoped, the way CI runs it:
```bash
pnpm exec turbo run build lint test --filter=cell-relay
```
`package.json` exists only so Turborepo can address this cell by name; the
scripts shell out to cargo. Because the cargo workspace currently *is* this cell,
those commands are workspace-scoped — a second Rust cell would need its own
package and its own scoped commands (`Cargo.toml` says so too).

## Not here yet

- **WASM bindings** (`wasm/`) —. Criterion benches exist in `agw-audit`
And `agw-guard`; is what turns the tech stack's budgets into
  CI-enforced measurements instead of local runs.
- **Drift detection, repair, and policy** — the rest of and +. The
Working reference for this cell's *shape* is still the TypeScript prototype in
 , which is not part of this workspace
And stays on plain npm.

---

*Cell boundaries:*

## Schema learning

`agw_inspector::inference::BaselineStore` learns nested JSON Schema shapes from
complete response captures, with optional `SpecPrior` shapes extracted from a
spec by the caller. No spec or service is required. Exact vendor/operation keys
isolate version histories. The first N valid samples publish a baseline;
subsequent observations return `Frozen`. Call `begin` to explicitly relearn,
retaining older versions. Reads return borrowed immutable baselines.
```rust
use std::num::NonZeroUsize;
use agw_inspector::inference::{BaselineStore, Key, Limits, Progress};

let mut store = BaselineStore::new(NonZeroUsize::MIN, Limits::default());
let key = Key::new("vendor", "GET /users");
store.begin(key.clone(), None)?;
let result = store.observe(&key, br#"{"data":{"email":"a@example.com"}}"#, false)?;
assert_eq!(result, Progress::Established { version: 1 });
# Ok::<(), agw_inspector::inference::LearnError>(())
```
After driving a `Tee` to completion, pass `tee.captured` and `tee.truncated`.
The learner reads raw JSON tokens, preserving integer/number shape without float
conversion. Also supplies a lossless `Inspection` tree; the learner
retains its standalone byte entry point. The original response bytes remain
owned by forwarding and are never rewritten by learning.

`Schema::json_schema` exports types/properties/items; `Schema::paths` exposes
escaped dot paths and pooled array paths (`data[].email`). Every node includes
presence count and confidence: presence once per response divided by samples.
A spec prior adds one pseudo-sample, so unsupported prior nodes decay in
confidence and live conflicting types form a union. Confidence measures coverage,
not safe-repair probability. Requiredness, enums and formats are not inferred.

Defaults bound each sample to 8 MiB, depth 64 (root zero), 4096 parsed/merged
nodes, 128 keys and 32 versions per key. Failed learning attempts leave existing
state intact; errors are learning outcomes, never drift verdicts. Callers must
handle them through availability containment when integrating this library.
This seam adds no alerts, persistence, OpenAPI loader or guard integration.
See runtime contract §10.11 for the exact rules.

## Lossless structured inspection

`inspect(captured, truncated, None)` now returns `Parse::Parsed(Value)` with
`numeric::Number` leaves. No number passes through a float. Serialization uses
`Number::lexeme`, retaining huge integers, long decimals, `-0` and `1e400`.
`None` selects `Precision::Observed`: oversized integers are retained as
`Unrepresentable`. `Some(Precision::HighPrecision)` explicitly models those
integers as `Verbatim`. Both serialize their original spelling.

`Tee::into_inspection` uses the default; use
`into_inspection_with_precision(Precision)` to supply a whole-document decision.
Adds `inspect_fields(captured, truncated, &FieldPrecisions)` for exact
field-local decisions. `Value::Object` contains
entry vectors, preserving duplicate keys so numeric evidence is never discarded.
The tree has a 128-node path-depth limit (root zero). Parse failures remain
recorded values, with capture-relative coordinates; truncation remains independent
of whether the retained prefix parses. No classifier or repair is added.

The legacy inspection tests only change call arguments and three expected
JSON-value constructions. Their byte-forwarding and failure assertions remain.
The new property suite compares raw numeric tokens against the tree and its
serialized output, using independent token scanning and generated nested JSON.

## Shape diff and classification

`agw_diff::compare(side, before, after)` compares typed `Snapshot`s;
`compare_inspection(side, baseline, inspection)` compares a selected baseline
with the lossless tree. Both have `*_with_limits` entry points. `Snapshot::from`
adapts a learned schema with `Evidence::Learned`, retaining no repair
confidence. `Node` describes types, properties, pooled items, and explicit
requiredness, units, enum values, null meaning and confirmed identity metadata.
Callers extract authoritative facts from specs/detectors; this is not a general
JSON Schema validator or an OpenAPI loader.

Each change retains its side, escaped dot path, kind, class and stable reason;
confirmed renames also carry the destination path. `Report::class` aggregates
semantic > unknown > shape, returning `None` for no findings. Inspect `skipped`
separately: incomplete captures, duplicate keys, parser/internal failures and
resource limits are not vendor drift. Complete malformed bodies are shape with
no repair. Learned presence differences remain unknown. Metadata changes need
explicit evidence; identical shapes alone cannot establish semantic safety.
No guard, forwarding, policy or audit transport invokes this library yet.
```bash
cargo test -p agw-diff
cargo bench -p agw-diff --bench shape_diff
```
The native benchmark gates nearest-rank p50 <50 µs and p99 <500 µs over 20,000
samples per branch after 2,000 warmup iterations. Its nested payment/20-line-item
fixture covers unchanged and changed inspection conversion/diff/report disposal.
Capture, parsing and setup are excluded. It writes environment and quantiles to
`target/ti11-shape-diff.json`; a failed budget exits nonzero. These are local
native measurements, not CI, WASM or end-to-end latency claims.

## Selective repair library

`agw_repair::repair(side, source, inspection, report, candidates, mode)` accepts
original capture bytes and their parsed inspection, a report and
explicit repair proposals. Use the frozen mode from the call context. The
`repair_with_limits` variant bounds input/output, nodes/depth, proposals, paths
and replacement metadata. Default limits: 8 MiB, 4096 nodes, depth 64, 64 proposals.
The bounded guard explicitly composes this seam; the default forwarder and
SDK paths do not automatically enable it.

A `Candidate` identifies a finding and proposes `Rename { to }` or `Replace`
(one raw JSON scalar token). `Evidence { detector, applicability }` comes from
the caller; the engine computes their minimum as per-repair confidence. Shape
requires ≥0.97, semantic ≥0.99, and strict requires ≥0.99. Unknown drift is never
repaired. Rename findings must have confirmed identity and the exact sibling
destination. Statistical identity evidence is available through `agw_diff::rename`. Explicit scalar
replacement is not value synthesis or numeric arithmetic; numeric type changes
are refused as semantic even when the input finding is mislabeled. Permits
only an actually empty string to literal null under an explicit semantic
`NullMeaningChanged`/`MeaningChanged` proposal at ≥0.99 confidence. All other
type changes remain unsupported or numerically fenced; independent guard
verification is still required before dispatch.

The engine validates the inspection against original raw lexemes, then patches
only selected key/scalar token spans. Whitespace, escapes, key order and all
unrelated numeric bytes remain exact. Pooled `[]` paths expand to individual
array elements; audit paths use concrete `[index]`. Absent optional fields are
skipped, while no matching target is rejected. Destination collisions, chains,
swaps and overlapping edits cannot apply. A rejected supplied plan is atomic;
observe and no-op plans also return `Cow::Borrowed` with no audit patch.

Handle `Outcome::failure` before interpreting the result. It records
incomplete/malformed/mismatched captures, duplicate keys, invalid reports and
resource failures separately from drift. Internal failures are availability
bypass inputs; a complete unparseable capture remains non-repairable and its
existing `malformed_body` finding still follows contract §4.4 policy. Resource preflight can reject before
copying an oversized report; the caller still owns that report. For ordinary
rejections, assessments retain reasons/confidence and `unresolved` retains
findings for the policy caller. Applied proposals remove only their findings;
unproposed drift remains unresolved. `Outcome::class` summarizes unresolved
drift; a missing class is not a write-integrity proof.

An applied result owns patched bytes. `Outcome::patch` exposes the generated
cross-cell `Patch` with `applied: true`, `selective: true`, concrete
`fields_changed` and `byte_identical: false`. `Outcome::audit(draft)` stamps this
patch, aggregate applied confidence and the execution mode into an owned draft.
It preserves the caller's decision and proof. Policy enforcement, write-integrity
verification and audit publication remain separate future work.
```bash
cargo test -p agw-repair
```
## Statistical rename evidence

`agw_diff::rename::match_fields(removed, added)` borrows typed histograms for
caller-selected sibling fields. Callers own vendor/operation/side scope, windows,
exact value labels and compatible numeric binning. Each field records present
counts and total opportunities, including absences. `distances(a, b)` exposes
PSI and natural-log Jensen–Shannon divergence independently.

A pair must pass PSI ≤0.1 and JS ≤0.02 and be the only plausible candidate
for both endpoints. Results follow removed-field order: `NoMatch`, `Ambiguous`,
or `Confirmed { added, distances, confidence }`. Names never contribute to the
score. Null/empty evidence cannot confirm identity; incompatible scalar types
or binning identifiers do not pair. Invalid counts, duplicate/overlapping names
and resource limits yield an explicit error before any result is returned.
Limits: 64 fields/side, 256 bins/field, 1 MiB total metadata.

Confidence decreases with divergence, low volume and sparse coverage using
[contract §10.15](/agindow-runtime-contract.md). It is deterministic evidence,
not a calibrated probability. A confirmed low-volume match can remain below the
repair gate. Callers assign a scope-unique shared identity to matched nodes
and supply the detector score to alongside independent applicability.
Semantic child checks and repair thresholds still apply. The integration test
`rename_evidence` exercises this handoff, exact-byte repair and observe mode.
No traffic collection, persistence, guard wiring or latency claim is included.
```bash
cargo test -p agw-diff --test rename
cargo test -p agw-repair --test rename_evidence
```
## Per-field precision snapshots

`contract_snapshot` v2 carries optional `high_precision` booleans. Compile one
selected tool with `agw_guard::snapshot::precision_for`, then pass its map to
`agw_inspector::inspect_fields`. Only exact numeric paths receive HighPrecision;
missing/false flags remain Observed and small integers retain the i64/u64 fast
path. Unflagged oversized integers still retain every original digit.

The learner records `overflow_samples` and publishes a hint after overflow in
at least two complete live samples. Arrays count once per sample; spec priors,
malformed/incomplete samples and prior generations contribute no evidence.
`with_learned_precision` copies hints into existing snapshot fields, preserving
explicit choices. `Schema::precision_paths` uses the reader's escaped paths,
including `\e` for empty keys; the legacy `paths` API is unchanged.

Map compilation is bounded to 4096 entries / 1 MiB of path text. Compile outside
forwarding and handle errors at snapshot loading/availability containment. No
registry delivery or default-engine wiring is added. See contract §10.18.

## Unit-conversion evidence

`agw_stats::unit::detect(path, baseline, observed)` borrows caller-selected finite
f64 windows. Scope them to one field/vendor/operation/side and tenant/volume
segment; do not silently round lossless numeric tokens into this API. Windows
are capped at 4096 samples and paths at 4096 bytes. Fewer than 20 samples per
window return `InsufficientEvidence`, distinct from `Stable(metrics)`.

`measure` exposes magnitude empirical Wasserstein-1, normalized Wasserstein and
baseline-decile PSI, including unequal windows. Both normalized Wasserstein
and PSI must reach 0.2 to return `Drift(finding)`. Signed median ratios within
2% of 100, 0.01, 1000 or 0.001 receive typed `UnitConversion` evidence; other
significant shifts remain semantic without a diagnosis. Zero baseline medians
cannot suggest a ratio. Sign-only flips are invisible to magnitude metrics.
See [contract §10.19](/agindow-runtime-contract.md) for exact formulas.

`finding.audit(draft, side)` stamps generated v1 semantic drift and optional
`unit_conversion`/detail, clears stale patch and proof claims, and retains the
caller-owned policy decision. This API has no payload rewriting or repair
proposal. It neither executes the §4 write block nor wires the guard, collects
traffic, publishes artifacts or establishes production calibration/latency.
```bash
cargo test -p agw-stats
```
## Cardinality and enum growth

`agw_stats::enumeration::compare(path, baseline, observations)` borrows a known
string vocabulary and timestamped observations for a caller-selected enum field.
Scope baselines/windows to one vendor, operation, side and tenant/volume segment;
this API does not identify enums in free text or IDs. Strings retain exact case,
whitespace and Unicode spelling. Empty strings are valid categories.

The report exposes baseline, observed and union cardinalities, plus sorted new
values with count, frequency over all observations and earliest window timestamp.
A replacement remains novel even when observed cardinality falls. Known-value
frequency shifts alone never trigger. A nonempty baseline, ≥20 observations,
and a novel value with ≥2 occurrences and ≥1% frequency produce semantic evidence.
Below-floor novelty and insufficient evidence remain visible and distinct.

`report.audit(draft, side)` requires semantic evidence, enumerates material
additions in changes and all novelty under `metadata.agindow_enum_growth`, and
clears stale repair/proof claims while retaining caller-owned policy. No wire
schema changes are needed. Limits: 256 baseline entries, 256 observed distinct
values, 4096 observations, 4096 bytes per string/path and 1 MiB total input text.
See [contract §10.20](/agindow-runtime-contract.md) for exact boundaries.

This is standalone evidence without persistence, first-ever histories, numeric
enums, guard wiring or policy execution. / still own configurable
floors, cross-detector weighting, alert routing and production validation.
```bash
cargo test -p agw-stats --test enumeration
```
## Completeness, type consistency and string fingerprints

`agw_stats::field::{completeness, coercion, charset}` compare two borrowed
`Option<&agw_inspector::Value>` windows for a selected field. Callers own complete
window extraction, segmentation and baseline selection. `None` means absent;
`Some(Value::Null)` means explicit null. No numeric f64 projection occurs.

`profile` exposes non-null completeness, JSON type counts and separate missing,
null, mathematical zero and empty-string counts. Comparisons return the path,
both profiles, sufficiency and typed semantic reasons. Completeness/sentinel
rate changes ≥0.2 are material. Type consistency requires a baseline type with
≥90% share and a loss of ≥0.2 share among non-null observations. Both windows
need ≥20 opportunities, non-null values or strings respectively. Thresholds use
integer counts so inclusive boundaries do not depend on float subtraction.

String profiles expose per-string character-class counts and whole-string lexical
formats: ISO date, ISO timestamp, month-name/day and digits. Class-rate changes
≥0.2 and recognized format emergence (baseline ≤5%, observed ≥20%) produce named
reasons. Format evidence contains the anchored pattern; these patterns describe
shape rather than calendar validity. Unicode/replacement/control changes describe
decoded text rather than identify a raw encoding. Other formats may be invisible.

Limits are 4096 observations/window, 4096 UTF-8 bytes per path/string/numeric token
and 1 MiB combined text. Containers count only as a top-level type. Invalid numeric
tokens and resource limits return errors even for sparse windows. Profiles and
reports own counts only; no payload mutation, guard, policy, registry or audit
publication is added. See [contract §10.21](/agindow-runtime-contract.md).
```bash
cargo test -p agw-stats --test field_detectors
cargo bench -p agw-stats --bench field_detectors
```
The native benchmark gates each detector's nearest-rank p99 <2 ms over 10,000
measurements after 2,000 warm-ups. Each fixed fixture has 128 samples per window:
integer→null, integer→string, and ISO timestamp→month/day. It includes validation,
both profiles, comparison and returned report drop; setup/parsing are excluded.
Environment, scope and quantiles are saved to `target/ti17-field-detectors.json`.
This is a local fixture gate, not a production, all-input or WASM latency guarantee.

## False-positive controls

`agw_stats::control` supplies exact-scope seasonal window selection, configurable
customer/field/signal floors, supplied-significance gates, weighted alert ranking,
and connection/day notification consolidation. An offline evidence-replay harness
requires 30 growing-traffic days; synthetic runs cannot qualify production.
See [controls and caller obligations](./CONTROLS.md). No guard wiring or production
calibration exists; the real production replay remains pending.

## Detector significance

`agw_stats::significance::{unit, enumeration, completeness, coercion, charset}`
wrap existing detectors with raw-window statistical tests and normalized effects.
Typed reports retain descriptive results and all tested features; `decision`
applies only to original local findings. Original detector APIs remain
unchanged. See [methods, sampling assumptions and usage](./SIGNIFICANCE.md).
No guard/default-engine integration or production calibration is implied.

## Detection router and inference accounting

`agw_stats::router` borrows diff/invariant findings, retains mixed handlers,
queues optional unknown-case investigation in a bounded local FIFO, and provides
per-call cost reservations, background grants/actual-cost receipts and audit-review
confusion counters. See [routing and caller obligations](./ROUTING.md).
No guard wiring, worker execution, durable delivery or provider billing enforcement
is included; `NullEngine` remains the default.

## Local statistical windows

`agw_stats::windows::Store` extracts one scalar/missing opportunity per selected
field from complete lossless inspections. Scope includes tenant, connection,
vendor, operation, side, JSON Pointer field, segment, selected window identifier
and generation. Strict pointers escape `~` as `~0` and `/` as `~1`; `/` selects an
empty property name. Arrays (including numeric index traversal), container leaves
and duplicate-key inspections are refused; there is no pooling policy. Literal
`*` is an ordinary property name, never a wildcard. Inspector precision paths
use a different syntax; callers map their selected fields explicitly.

Create a store with explicit `Limits`, then `record` a batch of `Selection`s with
a complete inspection and caller UTC timestamp. Admission preflights the entire
batch: any failure preserves all windows and readiness. `publish` freezes cold
samples once per generation and clears the live window. `snapshot` reports
unavailable, warming counts, or immutable `Arc` baseline/live windows once both
meet the configured floor. Detector sufficiency remains separate. `advance`
clears only the live window; relearning requires a new generation. No live drift
is ever promoted into the reference vocabulary automatically.

Snapshot windows expose `fields`, `enumeration`, `vocabulary` and `unit` adapters.
Unit projection is disabled unless explicitly authorized as `ExactInteger` with
the selected field's `Precision::Observed`; only integers in ±(2^53−1) project.
Any unsafe numeric sample makes unit input unavailable for that whole window;
its lossless type/completeness evidence and string categories remain available.
Pass precision hints explicitly even for small integers: inspector numeric
variants do not preserve every field hint. Decimal/exponent lexemes are retained
for field evidence but conservatively excluded from unit projection.

Sample caps reject further admission until explicit live advancement; scope and
accounted-byte pressure reject without eviction. Byte accounting includes fixed
record overhead and retained text, excluding allocator slack and externally held
snapshots. The caller owns retention/removal and external snapshot lifetimes.
`set_available(false)` exposes an outer provider/refresh fault locally while
retaining the baseline; snapshot acquisition invokes no provider, network, clock
or storage API. Guard execution, export and remote refresh remain,
and. See contract §10.26 and the three `windows_*` integration suites.

## Explicit statistical execution

[`agw_guard::statistical`](./STATISTICAL.md) now connects local windows, raw-window
significance, materiality, routing and the shared policy matrix on actual native
request/response calls. Activation is explicit; `NullEngine` stays the default.
The library preserves completed upstream outcomes on response halts and admits
no more than one optional local investigation job per call. Composes the
statistical evidence into one audit artifact; HTTP/SDK/streaming drivers,
default-engine wiring and WASM statistical budgets remain separate.

Adds bounded local window staging, lossless v1 projection, a durable native
background outbox and tenant-bound HTTPS transport. See [profile export](./EXPORT.md).
Qualified isolated staging export with synthetic observations;
the default guard does not collect or send.

Adds explicitly selected aggregate snapshot refresh, a bounded local cache,
revision/generation fences and separate native/WASM host transports. Historical
rollups never become exact detector windows. See [profile refresh](./REFRESH.md).
Qualified native/WASM refresh and recovery against staging;
no scheduler starts by default. See the.

Adds standalone [bounded call-level array sampling](./SAMPLING.md) with
explicit policy identity and conditional significance adapters.

Adds [precision-budgeted unit inputs](./PROJECTION.md), with explicit
exact/bounded policies and local error provenance.

Adds [cross-endpoint consistency](./CROSS_ENDPOINT.md): bounded paired-profile
candidates and workflow-scoped local observations, consumed through immutable
snapshots in the existing native guard. No peer fetch or automatic activation.

Adds an explicit background [qualification/activation controller](./ACTIVATION.md)
with staged native publications; default construction remains disabled and no
production calibration dataset is supplied.

Supplies [strict human review handoffs](./REVIEW.md): bounded off-path evidence, local outbox and replay retention; hosted delivery remains separate.

Adds the local [audit publication and retrieval](./AUDIT.md) seam:
honesty-validated artifacts for every decision (bypasses included), bounded
recurrence and append-only storage, a host-drained NDJSON sink outbox and
the tech stack OTLP/JSON metrics; the durable registry archive and its
opt-in authenticated read route are documented there (contract §10.40).

Adds native durable audit delivery: `Publisher::stage` transfers queued
artifacts off-path into `agw_audit::delivery::Outbox`, and caller-owned bounded
drains deliver through tenant-bound HTTPS or the existing NDJSON sink. Retry
reservations and counters survive restart; registry ingestion acknowledges stored,
duplicate and conflicting evidence explicitly. See [AUDIT.md](./AUDIT.md) for
limits, scheduling and producer configuration. No library scheduler or hosted
activation is supplied.

## Node SDK

The [local Node package](./node/README.md) builds ESM/CJS and the release WASM
guest. Default detection remains NullEngine; explicit scoped statistical windows
run. Package tests cover offline install, size, tool/outcome identity,
policy halts and faults. A CI-gated local agent-loop benchmark covers the default
engine only; retains integrated statistical latency qualification.

Extends this package with config-file resolution, immutable events,
optional OTel spans, explicit tool/client/fetch/Express adapters, offline
simulation/report and a limited Nango code generator. The
[Nango guide](./node/NANGO.md) states the exact surfaces and limitations.
Python parity, partner publication/adoption and hosted sandbox delivery remain
separate; the default engine still detects nothing.

## Local Python SDK

[python/README.md](./python/README.md) documents the typed uv-managed Python
decorators/context managers and identical shared-WASM binding. Build stages the
Node guest into the Python package; lint runs strict mypy; test runs Python
behavior/package fixtures and actual cross-language parity. CI gates default
NullEngine added p50 <100 µs separately from statistical paths. No PyPI release
or Python framework/HTTP adapters are claimed.

## Explicit HTTP and agent surfaces

The shared `agw-http-host` crate supplies a [local sidecar](./sidecar/README.md)
and [workers-rs host](./hosted/README.md). Connection bindings hold credentials;
execution envelopes carry handles. Exact six-field policy feedback and retained
response outcomes use the same shared Rust statistical path. NullEngine remains
the default. Local TCP/Unix and real workerd tests are explicit scripts; the relay
workflow additionally runs them. Deployment and public listings remain pending.
Node `guardRemote` and Python `guard_remote` implement explicit transport fallback
with one bypass event and the same upstream-enforced idempotency key.

## Embedded audit drain and measurements

Node publication is opt-in through `audit: { tenant }`. Completed guest calls
compose one frozen v1 artifact into a bounded FIFO; `drainAudit(sink)` is explicit,
ordered and off-path. Failed sinks retain the same ID. Capacity drops are counted;
retirement/restart loses guest memory. No guest network, timers or threads exist.
The host owns durable persistence and delivery. Wrapper §12.10 and contract §10.40
state the ABI and bounds.

[The embedded performance methodology](./node/PERFORMANCE.md) records the real
351-case statistical matrix, separate bypass/trap lifecycle measurements, strict
absolute/regression checkers and retained raw samples. Native timing cannot qualify
this Node boundary. Unmet budgets remain visible; no production SLA is implied.

## Credential evidence and cutover

[ local surfaces](CREDENTIALS.md) add bounded response evidence, a closed
credential-event boundary, tenant-owned health projection, opaque-binding canary
and overlap state machines, and the offline readiness report. These remain In
Review pending native execution and latency qualification. No minting, secret
custody, scheduler, default SDK integration or hosted qualification is claimed.

## Public benchmark protocol

[ methodology and standalone harness](./public-benchmark/README.md)
provide a synthetic conditional repair/policy safety development set and two
local baseline reproduction artifacts. Publication remains gated on private
held-out custody and external validator signatures; no Agindow entry is included.
