# Cross-endpoint consistency

Runtime the runtime contract adds explicitly selected cross-endpoint checks to the
Native guard. Defaults stay `NullEngine`. No network request, store lock,
remote registry or LLM runs during a guarded call.

## Correlation and scope

Every observation belongs to a caller-attested scope:
`(tenant, connection, vendor, workflow, entity, revision)`. The revision identifies
a stable business quantity, not merely a request ID. Change it when legitimate
business changes make old and new amounts incomparable. A qualified source key
also includes endpoint operation, request/response side, object-only JSON Pointer,
concept, unit and contract version. Equal names or distributions do not establish
identity. Customer selection asserts the conceptual relationship and common unit.

A cross-endpoint `Rule` declares distinct endpoint operations, a concept/unit,
absolute integer tolerance, maximum observation age and maximum timestamp skew.
Both time bounds are inclusive, in milliseconds; age is positive and at most
24 hours, and skew cannot exceed age. Values are exact i64, widened to i128 for
differences. Decimal/exponent tokens, unsigned values outside i64, arrays,
containers and missing values remain unavailable. No currency/unit conversion or
floating-point projection is performed.

## Profile discovery

`agw_invariants::cross_endpoint::discovery::discover` accepts up to 16 complete
field profiles, each with up to 64 samples. Samples carry explicit correlation
IDs, optional exact integer values and UTC Unix observation milliseconds. Concept
and unit tags must match. Correlation IDs must be unique per profile; all samples
must pair one-to-one, meet the configured tolerance/skew, and vary on both sides.
The minimum pair count is explicitly configured from 3 through 64. Constants,
missing values, incomplete profiles, mismatched identities/units or equal marginal
distributions with different pairings never establish a candidate.

Discovery returns up to 16 reviewable `Rule` candidates; exceeding the cap rejects
the batch rather than truncating it. Candidate IDs use input profile positions
(`cross-i-j`); customers assign stable IDs when accepting/versioning a selection.
Small paired samples suggest a relationship, not a production confidence claim.
Discovery never publishes, activates, pins, repairs or authorizes a write.

## Local observations and concurrency

`cross_endpoint::Store` is a bounded caller-owned administrative store. Use
`record(&[Observation])` to atomically publish a batch, and `snapshot(&scope)` to
freeze that workflow's evidence. Each qualified source retains its latest value,
sequence and UTC time. Sequences must be positive and strictly increase per
source; times must not regress. Duplicate keys in a batch, stale updates or
capacity exhaustion reject the entire batch. `value: None` explicitly invalidates
a prior numeric value. There is no fallback to an older successful sample.

The default is 16 scopes and 16 observations per scope; configured limits are
1–128 scopes and 1–32 observations per scope. Batches have at most 32 entries.
Identifiers are nonempty and at most 256 bytes. A non-waiting `try_lock` returns
`Busy` on contention or `Poisoned` after a publication fault. Administrative
callers may retry outside the guard. Snapshots remain immutable after concurrent
updates or `remove(&scope)`. Externally retained snapshots have caller-owned
lifetimes. Retention/removal is explicit; no eviction silently
changes a live snapshot.

## Native guard usage

```rust,no_run
use agw_guard::{cross_endpoint::Selection, statistical::{Engine, Plan}};

// `identity` comes from the trusted host; accepted rules are customer-selected.
let plan = Plan::new(identity, vec![])?
   .with_cross_endpoint(Selection::new(rule_version, contract_version, accepted_rules)?)?;
let engine = Engine::statistical(plan);

// Acquire local evidence before entering the guard; this never fetches a peer.
let snapshot = observations.snapshot(&workflow_scope);
let result = engine
   .begin_call(context, operation, audit_ref, work_budget, 0)
   .with_workflow(snapshot, current_sequence)
   .dispatch(request_body, None, original_operation_once, response_capture);
```

Selections validate versions, rule bounds and decoded pointers administratively.
At most 16 rules apply to an operation; every selected rule must name the plan's
operation on one side. Each field uses standard pointer escapes, at most 256 bytes
and 32 components. Request and response sources remain distinct. Combine local
And cross-endpoint rules in the same plan; contract versions must agree and
rule IDs cannot collide. Invalid attachments reject before runtime. Customers can
pin an immutable selection version, override a rule in an explicit replacement
selection, or disable it by omitting it. Nothing automatically upgrades a plan.

The guard freezes the plan, mode and supplied snapshot for one call. On each
configured phase it extracts the current endpoint, replaces that endpoint's cached
value for the comparison, and looks up only the exact qualified peer. It checks
sequence/time against any cached current observation. Regressive current calls
are `OutOfOrder`; they never publish a replacement. Missing current fields
explicitly replace old values with unavailable evidence for that call.

Both endpoint observations must share the full scope, concept, unit and contract
version, be nonfuture relative to the current capture timestamp, remain within
maximum age and maximum inter-observation skew, and carry exact integers.
Successful checks are `Pass`; confirmed mismatches are `Divergence`. `Partial`,
`Unsupported`, `Stale`, `Future`, `Skew`, `OutOfOrder` and scope mismatches are
unavailable evidence, never semantic alarms. A missing workflow attachment has
partial evidence. Supplied busy/poison/corrupt snapshots become availability
bypasses through existing containment, with one bypass per call and original
outcome identity preserved.

Divergence becomes a named semantic invariant finding, sharing routing and
Policy with local/shape/statistical findings. Observe passes with evidence;
repair reads degrade loudly and writes/unknown operations halt; strict retains
its explicit opt-in escalation. A request halt dispatches zero times. A response
halt preserves the completed original attempt; it cannot undo a write.

## Observation handoff and audits

`Phase.cross_endpoint` retains typed comparison reasons/provenance.
`Phase.cross_observations` returns deduplicated current observations for explicit
publication after the call. The guard never changes the shared store. A blocked
request has no publication handoff, while its attempted value stays in audit
comparison evidence. Hosts publish only observations appropriate to actual
transport outcomes; request evidence describes the observed request and does not
prove a write committed. The API cannot infer success from a generic host result.
Hosts must invalidate failed/incomplete captures or advance revision; otherwise
an older observation can remain eligible until its configured freshness limit.

Generated v1 per-phase invariant drafts retain `cross_endpoint_diverged` changes,
actual/expected values and no repair/write-integrity claim. Open metadata under
`agindow_cross_endpoint` includes the selected rule/contract versions, full
workflow scope/revision, both configured endpoint operations/sides/fields (even
for a missing peer), observed values/timestamps/sequences, concept/unit, numeric
and time bounds, and unavailable reasons. Headers/warnings and final policy remain
's. Hosts own publication and final call-wide audit composition; no frozen
wire schema changes or fourth drift class are needed.

The implementation acquires evidence from earlier explicitly published workflow
observations. It does not fetch endpoints, infer entity identity from payloads,
wait for concurrent calls, persist remotely or establish global transaction
consistency. Legitimate updates and eventual consistency need a host-selected
stable revision and appropriately comparable observations; time limits alone do
not prove business causality.

Validation: `cargo test -p agw-invariants` and the guard's `cross_endpoint` /
`cross_endpoint_edges` integration suites. The native invariant latency gate
`cargo bench -p agw-guard --bench invariants` includes six local forms on both
phases plus one paired-endpoint request check, frozen snapshot clone, projection,
freshness/correlation, observation handoff, policy and generated audit drafts.
Store acquisition, setup, network, streaming and WASM are excluded. The report
`target/ti22-invariants.json` retains per-fixture measurements and environment;
it is a local fixture gate, not a production or maximum-workload guarantee.

Planned follow-ups: background peer acquisition
and opt-in qualification/activation.
Both start at Todo; existing explicit observation publication and selection remain supported.
