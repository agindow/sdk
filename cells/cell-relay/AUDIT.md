# Audit publication, delivery and retrieval

Every call publishes exactly one artifact conforming to the closed
Schema — clean,
observed, degraded, repaired, bypassed or blocked. A bypass is evidence too, so
nothing filters by decision. Publication is a local, best-effort pipeline in
`agw-audit`: validate honesty, stamp recurrence, append to an append-only
bounded store, enqueue once for a host-driven sink outbox and project the
the tech stack metrics. No call awaits delivery, and the only outcome that
stops a publication is local store refusal — callers may ignore it. The
in-path guard never depends on this crate (contract §10.40); owns
composing statistical evidence before publication.

## Honesty rules the schema cannot express

`artifact::validate` rejects, before storage or publication:

- `patch.applied` and `patch.byte_identical` not complementary (a repair always
Breaks byte identity — contract §3.1);
- an applied patch that names no changed field;
- a `repaired` decision without an applied patch;
- `occurrences` or `first_seen` present without the other;
- tenant metadata that is not a bounded identifier (≤256 bytes).

## Recurrence

`Recurrence::new(capacity)` tracks one signature per
`(class, side, vendor, sorted changes)`, local to a tenant and process. A first
observation stamps `first_seen` and `occurrences = 1`; repetitions keep
`first_seen` and increment the count. Default capacity is 4,096 (ceiling
1,000,000). A full map refuses the new signature without evicting history or
mutating the drift — the artifact still publishes, unstamped, and
`Publisher::recurrence_dropped` counts it. Callers own any merge across
instances; this is not the corpus and no global count is claimed.

The publisher stages one signature update and commits it only after storage
admission. Refused artifacts do not change recurrence counts, first-seen times,
signature capacity or recurrence-drop counters.

## The local store

`Store::new(tenant, limits)` is append-only and bounded. Identity is the
artifact ID: an identical repetition is an idempotent `Append::Duplicate` that
is neither re-stamped nor re-delivered, and different bytes under one ID is
`Error::Conflict`. Capacity or byte ceilings refuse the new record rather than
evicting evidence. Retention is measured from the artifact timestamp at append;
an already-expired artifact is refused, expiry hides a record from `get`, and
`purge_expired` is the only removal path (host-run, off-path). `export_all`
returns retained evidence in append order for enterprise export.

| Bound | Default | Ceiling |
|---|---|---|
| Records | 65,536 | 1,000,000 |
| Serialized bytes | 64 MiB | 512 MiB |
| Retention | 90 days | 1 hour–3,650 days |

## Publication, sinks and metrics

`Publisher::new(tenant, config)` combines the store, recurrence map and a
bounded FIFO outbox (default 1,024, ceiling 65,536). `publish` decides
idempotence on the *received* evidence with publisher-owned recurrence fields
normalized away, so replaying the unstamped original against the stamped record
is a repetition, not a conflict; a genuinely different body under the same ID is
`Error::Conflict`. A full outbox drops delivery (`Publisher::dropped`), never
storage. `drain(&mut sink, limit)` delivers in order; a sink failure keeps the
event at the front and stops the drain. `NdjsonSink` implements the customer
log-sink shape: one compact JSON artifact per line. `Metrics::record` counts
`drift_events_total{class,side}`, `repairs_total{outcome}`,
`policy_violations_total`, `bypassed_total` and `blocked_writes_total`;
`guard_latency_p99` is caller-supplied and absent unless measured.
`Metrics::export_json` renders a deterministic OTLP/JSON `ResourceMetrics` document
and performs no network I/O.

A direct drain to a sink declaring another tenant delivers zero without sink
I/O, retaining the queue and delivery counters. This fence also covers artifacts
without tenant metadata; unscoped sinks such as `NdjsonSink` remain supported.

## Composed statistical evidence

`agw_guard::statistical::Outcome::compose(draft)` produces the one artifact this
publisher consumes. It keeps 's decision and frozen mode, refuses stale
draft repair/hypothesis/integrity claims, and replaces the reserved metadata
keys. A semantic response halt remains `blocked` with `drift_halted` detail
even when an earlier malformed write request passed through; actual availability
faults retain bypass precedence. The bounded `metadata.agindow_statistical` subtree
(`agindow_statistical/1`) carries typed per-test findings, multi-entry
hypotheses, generation/readiness/suppression provenance and explicit truncation
(256 findings, 1,024 changes, 256 KiB of serialized findings). `publish` on the
same outcome forwards through `Publisher::publish` and returns its non-blocking
result or `None`; a store refusal, full outbox or failing sink never changes the
upstream outcome, the decision or policy, and composition never awaits delivery.
The shared fixture

round-trips through the frozen schema. See [STATISTICAL.md](./STATISTICAL.md)
and runtime contract §10.41.

## Durable archive and retrieval (the registry)

Migration
adds a Postgres table whose trigger rejects every `UPDATE`, `DELETE` and
`TRUNCATE` outside `purge_expired_audit_artifacts(at)`. Appends go through
`append_audit_artifact` under one advisory lock per `(tenant, id)`: an identical
replay returns `duplicate`, a conflicting body raises, and retention is fixed
per record at append. `AuditArchive` wraps that store with the same honesty
model and adds a bounded enterprise export (≤10,000 artifacts, one ≤366-day
window, oldest first, expired evidence excluded) and an explicit purge.

`GET /v1/audit/:id` runs on the profile worker and is disabled until tenant-bound
reader credentials exist (`AUDIT_READERS`, SHA-256 digests exactly like
`PROFILE_READERS`). It accepts one percent-decoded path segment (≤512 bytes, no
controls or slashes), authenticates before lookup, answers unknown or expired
IDs with 404, serves `no-store`, treats contradictory tenant metadata and
storage faults as 503 and never returns another tenant's record. Retrieval never
rewrites a record.

## Native delivery and caller-owned scheduling

`agw_audit::delivery::Outbox` is a native durable spool bound to one tenant and
one destination. Allocate a separate private path for the registry and each
customer sink. One process owns a path; repeated canonical paths in that process
are rejected. Do not share files between processes. Share an `Outbox` between
threads only through a host mutex; its exclusive mutable drain cannot send the
same queued item concurrently. No library starts a timer or thread.

At a caller-owned interval (for example, one second), run this on the host's
background worker, outside guarded calls:
```rust,ignore
// Setup once: private parent directory must already exist.
let mut spool = Outbox::open(path, "acme", Limits::default())?;
let mut sender = HttpsSender::new(
    "https://registry.example/v1/audit", "acme", producer_token, 5_000,
)?;
// One host tick; recover errors here, never propagate them into a guarded call.
publisher.stage(&mut spool, 32)?;
spool.drain(unix_time_ms, 32, &mut sender)?;
let delivery_metrics = spool.metrics().export_json(Utc::now());
```
Each staging/drain tick is capped at 1,024 items. `stage` removes a publisher
front only after fsync/atomic-rename admission succeeds. Release publisher ownership before network delivery. Filesystem work is
off-path; if a host shares a publisher with guarded calls, use nonblocking
admission and a caller-owned contention/loss counter so staging never makes a
call wait for a publisher mutex. A full spool retains the publisher front; the bounded
publisher outbox counts rejected new deliveries in `dropped` and
`audit_publication_drops_total`. The spool defaults to 1,024 entries/64 MiB,
with hard ceilings of 65,536 entries/512 MiB, including JSON escaping, retry
metadata and reserved counter growth. One artifact is capped at 1 MiB.

The spool persists attempt reservations before sends. Defaults are eight
attempts and exponential backoff from one second to one hour. A failed head
stops the tick; later entries cannot pass it. Exhaustion drops the head only
after its deadline. Conflicts are terminal, counted losses. An interrupted snapshot uses a single private temporary slot, reclaimed on reopen.
Reopen after any
persistence error: the last rename may have succeeded despite a directory-fsync
failure. Restart replays identical evidence; registry idempotency converges an
acknowledgement lost after storage. Pending publisher memory is not crash-durable
until the host stages it. Successfully staged artifacts survive process restart.

Use the same `spool.drain` with `NdjsonSink` for customer logs; framing remains
one compact JSON artifact per line. Supply a nonblocking or timeout-bounded
writer: the synchronous `Sink` trait does not interrupt blocked writes. Sink
delivery is at least once, so a sink must tolerate replay (partial writes can leave a partial line on failure).
`DeliveryMetrics::export_json` projects durable cumulative deliveries, retries,
drops, admission rejections, conflicts, drain count and elapsed nanoseconds as
OTLP/JSON. Hosts scrape or export both publisher and delivery metrics; no network
exporter is started.

### Registry producer route

Set the secret `AUDIT_PRODUCERS` to a JSON array of up to 256
`{ "tenant": "acme", "sha256": "<64 lowercase hex characters>" }` entries.
Digests must be unique. Hash the bearer token; never put the token in source,
URLs, artifact metadata or command-line arguments. Producer and reader maps are
independent. Absent producer configuration keeps ingestion disabled (503).

`POST /v1/audit` accepts the unchanged frozen v1 artifact JSON body, a bearer
token and `X-Agindow-Tenant`. The native HTTPS sender follows 's curl-stdin
credential convention, disables curl config loading and redirects, enforces
HTTPS and a 1..30,000 ms timeout, and buffers only curl's status code. Endpoint,
tenant and token are configured together; the spool checks the sender tenant
before any I/O, including artifacts without tenant metadata.

| Status | Meaning |
|---|---|
| 201 | Stored durably |
| 200 | Exact JSONB duplicate; no second record or recurrence stamping |
| 409 | Immutable ID conflict; counted terminal loss |
| 401 | Missing/invalid bearer token |
| 403 | Missing/unknown/mismatched tenant header or artifact metadata |
| 413 | Request exceeds 1 MiB |
| 400 | Invalid UTF-8, JSON, schema, honesty, unaddressable ID or expired evidence |
| 503 | Disabled/malformed producer configuration or storage fault |

Only 200/201 acknowledge delivery. Every other status except 409 retains the
head under its bounded retry budget. Authorization and streamed byte bounds
precede archive construction/mutation. Storage always uses the authenticated
tenant; no request field selects another tenant. Responses use `no-store`.

### Verification

Review corrections verified on October 20, 2026: seven `publish_edges` tests
and two `statistical_audit_policy` tests cover refusal-safe recurrence, direct
tenant fencing and malformed-request/response-halt precedence. The full
`pnpm check` gate passed all 13 tasks, including 592 Rust tests and doctests;
workspace Clippy and the native availability latency gate also passed. Rust
runtime checks used offline Rust 1.99.1 Linux containers with serial harness
scheduling to avoid existing timestamp-based temporary-file collisions;
explicit concurrency tests retained their worker threads.

Five isolated mutations were rejected by the new tests: hiding a halt in the
decision, hiding it in the detail, removing the direct tenant fence, committing
recurrence before storage and counting a recurrence drop before storage. The
mutation source copy was restored after each check; repository sources were
never mutated by these checks.

`cargo test -p agw-audit --test delivery --test delivery_edges --test delivery_persistence` covers ordering,
restart, ambiguous acknowledgements, pressure, tenant fences, concurrent drains,
NDJSON and persistence faults. Transport unit tests fake curl and inspect stdin
and argv. `tests/audit-ingress.test.ts` covers the auth/validation matrix;
`tests/audit-delivery-runtime.test.ts` runs the production Worker in workerd
against real PGlite append, duplicate, conflict, retrieval and retention purge.
Regenerate its shared publication fixture with:
```bash
cargo run -p agw-audit --example publish_audit > cells/cell-relay/crates/audit/tests/support/relay-audit.json
```
A Rust test locks the fixture to the real publisher's stamped output.

## Limits

Local implementation only. Statistical composition is supplied by
 (see
[STATISTICAL.md](./STATISTICAL.md)); default-engine wiring remains future work.
Native delivery and registry ingestion are implemented by
; the embedded WASM/SDK drain
boundary is; hosted
qualification is. The registry
route is off by default; enabling it and applying migration 004 to staging are
deployment steps, not claims made here.

Contract: [§10.40](/agindow-runtime-contract.md#1040-ti-29--audit-artifacts-recurrence-and-durable-retrieval).
Tests: `cargo test -p agw-audit`,
`cargo test -p agw-audit`.
