# Local detection routing

`agw_stats::router::route(&report, &invariants)` borrows findings and
caller-confirmed invariant evidence. Each dispatch exposes the original evidence,
internal label and selected handler:

| Internal label | Handler | Tier 2 eligible |
|---|---|---|
| Shape | Selective repair | No |
| Semantic | Policy | No |
| Invariant | Verification | No |
| Unknown | Conservative policy | Yes |

Mixed reports retain every dispatch. Numeric type changes retain 's semantic
classification; this layer does not infer repair safety. Skipped reports expose
`unavailable` and no dispatches. A complete empty report has neither findings
nor unavailability. No handler executes here, and routing does not gate 's
predeclared statistical family.

## Optional background investigation

Create one `CallBudget::new(tenant_scoped_audit_ref, maximum_micro_currency)` per
actual call and share it across request/response phases. Explicitly submit unknown
routing with `outbox.submit(&routing, &mut call, job_ceiling)`. Only one job per
call can be admitted. Known/clean/skipped inputs reserve nothing. Full/disabled
outboxes and exhausted budgets return typed admission outcomes; local dispatches
remain available. No callbacks, worker invocation, I/O or futures run in submission.

An `Outbox` is a caller-owned in-memory FIFO, capped at 1024 queued jobs; zero
disables it. Its jobs contain only a bounded audit reference and cost accounting.
The audit reference must identify evidence the background worker can retrieve.
Use `pop` outside the request path and separately bound worker concurrency.
There is no durable delivery, restart deduplication or automatic retry.

All costs use integer micro-units of one caller-configured currency. Admission
reserves a per-call upper bound; it is **not a measurement of billed cost**.
A worker calls `job.authorize(maximum_cost)` before each inference, using price
and token limits which actually bound that provider request. Successful grants
consume the job cap, including unsettled grants; unused amounts are not recycled.
A worker consumes each grant with `grant.settle(actual_cost)` to produce a receipt
with the audit reference, authorization and actual cost. Export and aggregate
receipts by that reference for measured per-call spend. In-path inference cost is
zero because neither routing nor submission can execute inference.

`receipt.overrun` reports actual charges above authorization without clamping
them. Provider enforcement, receipt export and handling overruns belong to the
worker. This library cannot guarantee an external provider's bill, recover lost
receipts or measure actual charges without the caller reporting them.

## Audit-review accuracy

`Reviews::record(predicted, truth)` accepts the four internal labels. `count(truth,
predicted)` exposes a fixed confusion matrix, `reviewed` exposes its denominator,
and `accuracy` returns `None` until a review exists. `from_counts` restores
validated counters. Overflow rejects the entire update without modifying state.

Only record ground truth from explicit audit review. Hosts must correlate labels
to evidence and deliver each review once; no persistent ingestion/deduplication or
production accuracy claim is included.

## Bounds and integration

Routing validates at most 4096 combined diff/invariant findings, 4096 bytes per
path/rule and 1 MiB total text before allocating dispatches. Audit references are
nonempty and at most 4096 bytes. Queue operations do not copy customer bodies.
Errors are local `Result`s for future availability containment.

Runtime contract [§10.25](/agindow-runtime-contract.md) defines the seam.
Owns guard integration;
Owns policy, invariant execution, and hosted investigation.
The statistical guest is default; configure windows to detect. No wire schema or latency guarantee is
introduced.
```bash
cargo test -p agw-stats --test router --test router_cost --test router_review
```