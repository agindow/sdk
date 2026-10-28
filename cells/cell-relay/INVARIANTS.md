# Standalone invariants

`agw-invariants` implements runtime contract §10.30. Adds explicit native
guard execution (§10.31), described below. Standalone compilation/evaluation
remains independent of the guard.

The DSL is typed Rust, with six forms:

| DSL | Meaning |
|---|---|
| `Relation { left, relation, right }` | Exact integer `=`, `<`, `<=`, `>`, `>=` |
| `Range { field, min, max }` | Inclusive integer bounds |
| `Enum { field, values }` | Known string vocabulary, extensible under addition |
| `Ordering { earlier, later }` | Integer ordering, including normalized epochs |
| `Sum { total, terms, tolerance }` | Absolute difference from sum at most tolerance |
| `CrossEndpoint { left, right, tolerance }` | Absolute difference at most tolerance |

Compile outside the request path. Names resolve to positional slots once; each
compiled rule holds a specialized function pointer and prepared operands, without
an AST. Evaluation accepts a borrowed slice, uses bounded arithmetic/string
comparisons, and allocates nothing. There is no text parser.
```rust
use agw_invariants::{compile, Form, Rule, Scalar, Verdict};

let rules = [Rule {
    id: "gross-total".into(),
    form: Form::Sum {
        total: "total".into(),
        terms: vec!["amount".into(), "tax".into()],
        tolerance: 0,
    },
}];
let program = compile(&rules, &["total", "amount", "tax"])?;
const VALUES: [Scalar<'static>; 3] = [
    Scalar::Integer(120), Scalar::Integer(100), Scalar::Integer(20),
];
assert_eq!(program.rules()[0].evaluate(&VALUES), Verdict::Pass);
# Ok::<(), agw_invariants::Error>(())
```
Caller projections must use exact i64 quantities in consistent units. Sums and
differences widen to i128, including opposite signed extremes. Decimals, booleans,
null, missing fields, incomplete bodies and unsupported numeric representations
must become `Scalar::Missing`; never round a value into an invariant. Temporal
values need caller-validated common integer epochs. New enum strings produce
`EnumAddition`, not `Violation`; this does not authorize a write. Unsupported
input produces `Unavailable`, never success. Preserve binding positions between
compilation and calls. For cross-endpoint checks, callers provide already
correlated evidence in the same slots; the library cannot attest freshness,
identity, units or whether two quantities are conceptually comparable.

`store::Store` is caller-owned, with one instance per tenant. It keeps immutable,
strictly increasing positive versions per `(vendor, operation, contract_version)`.
Publication rejects invalid definitions atomically. `select` takes an explicit
version and `Controls`: an optional version pin, rule replacements by existing ID,
and disabled IDs. Unknown/duplicate IDs and conflicting override/disable controls
are errors. A pin selects history within the exact scope and contract version.
The selected snapshot retains its scope, version and controls; later publications
cannot change it. Retain snapshots for runtime evaluation. Selection allocates
and recompiles administratively. No implicit latest-version fallback, eviction,
remote registry dependency or persistence exists.

Hard bounds: 128 rules, 128 bindings, 32 sum terms, 128 enum values, and 256 bytes
per identifier or enum string. Enum strings may be empty. Runtime oversized enum
strings are unavailable. Stores accept 1–128 scopes and 1–128 versions per scope
(default 16 each). Exhaustion rejects publication without discarding history.

`learning::derive` accepts at most 128 complete caller-owned profiles: exact
numeric min/max and full string vocabularies with sample counts. Empty/incomplete
profiles contribute no candidates; malformed complete profiles reject the batch.
Candidate IDs equal their unique field binding names. The library does not select,
publish or activate candidates automatically. Ranges describe observed data,
not promised business limits. Marginal profiles cannot establish arithmetic or
cross-endpoint identities; those rules require explicit customer evidence.

Validation: `cargo test -p agw-invariants`; the workspace gates also compile
rustdoc and lint every target. Compilation/store/learning errors are administrative
`Result`s; no default engine or availability behavior changes.

## Explicit native execution

Attach frozen snapshots to the existing `agw_guard::statistical::Plan`. Statistical
fields may be empty when only invariants are configured. No default engine changes.
```rust,no_run
use agw_diff::Side;
use agw_guard::{invariant::Selection, statistical::{Engine, Plan}};
// `identity` is the trusted tenant/connection/vendor/operation/segment scope.
// `request_snapshot` and `response_snapshot` come from `Store::select`.
let plan = Plan::new(identity, vec![])?
    .with_invariants(Side::Request, Selection::new(request_snapshot)?)?
    .with_invariants(Side::Response, Selection::new(response_snapshot)?)?;
let engine = Engine::statistical(plan);
// engine.begin_call(context, operation, audit_ref, work_budget, inference_budget)
//     .dispatch(request_body, outbox, operation_once, response_capture)
```
Snapshots retain compilation binding order. For guard integration, every binding
must be an object-only JSON Pointer (256 bytes, at most 32 decoded components),
with standard `~0` and `~1` escapes. `/` selects an empty property name. Arrays
are unavailable, including `/items/0`. A selection rejects malformed pointers
administratively; attaching rejects mismatched vendor/operation or duplicate phase
attachments. Own stores per tenant and choose contract versions explicitly. Calls
freeze the plan and mode, so replacing a plan cannot change an active call.

Complete captures are parsed once by the lossless inspector, then validated for
duplicate keys and existing native node/text bounds. The shared cooperative work
budget is charged before parsing or building evidence. Each selected rule gets a
record containing its ID, verdict and actual/expected evidence. Exact sum values
are computed in i128; tolerances are absolute input units. No numeric formatting,
timestamp parsing or sampling grants permission to project decimals. Incomplete
captures produce no invariant findings; missing/wrong-type/array/numeric evidence
is unavailable per rule and cannot suppress another rule's confirmed violation.
Cross-endpoint operands are caller-correlated fields in the supplied document;
Still owns acquiring and validating cross-endpoint evidence.

| Mode | Read finding | Write/unknown finding |
|---|---|---|
| observe | Pass with warning/evidence | Pass with warning/evidence |
| repair | Degraded, original outcome | Halt |
| strict | Halt (explicit opt-in, contract §4.2) | Halt |

An enum addition is a separate informational `enum_expanded` semantic finding.
It never becomes `invariant_violated` and still halts unproven writes in repair
and strict modes. Vocabulary alone cannot detect a same-label meaning change;
explicit arithmetic/relational constraints can independently detect its effect.
No candidate learning or automatic repair is enabled.

Routes invariant violations internally to verification, then the shared
policy matrix sees their confirmed semantic class with repair unavailable. Enum
additions route as semantic diffs. Mixed shape/statistical/invariant findings
survive and the strongest policy decision wins. A request halt dispatches zero
times. A response halt retains the original completed outcome; it cannot prevent
or roll back a write already executed. Caller code and response projection remain
outside engine recovery. Internal faults preserve upstream outcome identity,
skip later work, and count one bypass per call through the shared readiness and
three-failure cooldown. The statistical guest remains the default;
`NullEngine` is only an explicit native opt-out.

Each configured phase returns `Phase.invariants`, `unavailable_invariants`,
`invariant_audit`, `headers` and `warning`. Audit drafts use the generated frozen
v1 schema: semantic class, named rule IDs in change paths, `invariant_violated`
or `enum_expanded` kinds, observed values in `from`, computed expected values,
operators/bounds/tolerances in `to`, and metadata under `agindow_invariants`.
Metadata retains verdict/severity, unavailable count, tenant/connection, contract
and invariant versions and pin/override/disable controls. IDs append `.request`
or `.response` to the host audit reference. `proof.verified_against` is `pin` for
pinned selections and `cache` otherwise; this is local selection provenance.
`write_integrity` stays false, and patch evidence records unapplied, byte-identical
payloads. Passing a suite does not prove a write correct. These are invariant-only
per-phase drafts; the full mixed reports remain in `Phase.report`/`records`.
Can compose this evidence into the per-call statistical artifact when a
call runs the statistical engine; hosts own other composition, persistence,
metrics and delivery of headers and warnings. No frozen contract changes or
fourth wire drift class are needed.

Unavailable rules are counted explicitly. With no finding, an unavailable suite
has a bypassed draft/status and warning rather than a clean claim; this is not an
engine fault or a global bypass count. Clean completed suites have an `ok` draft.
When another finding blocks or degrades the phase, its aggregate decision also
determines the status header; unavailable invariants do not overwrite that status.
Malformed-body policy remains; incomplete captures and internal faults have
no invariant draft. Faulted phases never retain partially constructed evidence.

`cargo test -p agw-guard --test invariant --test invariant_faults --test invariant_selection`
checks integration. `cargo bench -p agw-guard --bench invariants` gates native p99
below 100 µs on four fixed six-form both-phase fixtures: clean, read violations,
observe enum additions and response write halt. Timing includes lossless parsing,
validation, extraction, evaluation, routing/policy, generated audit drafts, headers,
containment and result drop; setup, I/O, streaming and WASM are excluded. Reports
include environment and nearest-rank percentiles at `target/ti22-invariants.json`.
This local fixture gate is not a production latency guarantee for maximal suites.

Adds [workflow-scoped cross-endpoint observations and consistency](./CROSS_ENDPOINT.md),
with paired-profile candidates, explicit correlation/revision, immutable local
snapshots, age/skew fences and native guard/audit integration. The earlier
cross-endpoint DSL form still supports caller-supplied slots independently.

Now provides optional background [qualification and activation](./ACTIVATION.md).
Explicit selection remains supported; profiles and candidates never authorize
activation without customer policy and independent qualification evidence.
