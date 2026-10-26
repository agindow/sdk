# Selected profile refresh
Background synchronization implements [contract §10.22](/agindow-runtime-contract.md)
using frozen.
This is an explicit library/host boundary; no background thread starts by default.
`NullEngine` remains the default. Aggregate snapshots cannot make the statistical
guard ready: `exact_detector_inputs` always returns unavailable. Exact
windows and the detector provider retain their existing lifecycle.

## Selection and evidence

Pass an exact -selected scope, generation, grain, UTC bucket and profile
start/end to `agw_stats::refresh::Cache::select`. Each admission is one selected
field/window, not a claim that an entire seasonal family is ready. The selection
must use 's canonical segment tuple: `["ti48", connection, generation,
segment, window]`. `volume_cohort` remains a trusted explicit scope dimension.
Unknown legacy segments are refused. The refresh service never chooses latest,
alters selection or automatically relearns. A selected rollup that has changed
its bounds becomes unavailable until the selector explicitly chooses it again.

`Snapshot::aggregate` retains the HLL/value/numeric/character histograms,
excluded-number count, contributing window count and revision. HLL is approximate;
numeric bins are intervals and excluded numbers remain excluded. These summaries
lack exact vocabularies, lossless numbers, sample timestamps, detector confidence
and repair proof. There is no conversion into an exact `windows::Snapshot`.

## Background owner and local call boundary

1. Construct a bounded `Cache` and explicitly admit selected slots. Save returned
   IDs. Startup makes those slots due immediately.
2. On a background timer, call `begin(monotonic_ms)` to detach a `Job`. At most one
Job per selected slot can be in flight. No mutex/cache borrow spans network I/O.
3. Fetch the job using a transport below. Pass its bytes or typed failure to
   `complete(job, result, monotonic_ms)`. The cache checks deadline, selection
Epoch, full scope, generation, version, provenance, counters and revision before
Replacing one immutable `Arc` in local memory.
4. On an invalidation hint, call `notify(id, monotonic_ms)`. Hints coalesce; they
Cannot reset retry backoff or force another concurrent fetch. Notifications are
Caller-owned, optional hints; the recovery timer remains necessary.
5. At call start, `read(id, monotonic_ms)` performs a bounded local map lookup and
   `Arc` clone. It accepts no transport, async callback or storage handle. Retain
The returned handle for the call. A missing/expired snapshot is unavailable;
Use existing guard availability handling. Do not mark aggregate inputs ready.

Choose a poll interval shorter than `refresh_ms`; successful refresh cadence is
bounded by refresh period + owner polling lag + transport duration. This is an
owner scheduling requirement, not a hosted SLA. Slow or absent owners cannot
silently extend TTL. `begin` notices abandoned reservations and schedules retry;
a late result cannot complete a newer reservation. Read time never launches work.

`Limits` validate: 1–4096 slots; 1 byte–64 MiB accounted retained cache bytes;
TTL 1–3,600,000 ms; nonzero refresh interval strictly below TTL; timeout 1–30,000 ms;
backoff/recovery 1–3,600,000 ms; 1–16 attempts per failed cycle. Admission/capacity
failure rejects new state; no eviction or baseline promotion occurs. Each failure
uses exponential backoff capped at an hour. Exhaustion waits `recovery_ms` before
starting a new cycle. A success resets failures. Cache byte accounting covers
selection/aggregate retention, not outstanding call-owned handles, allocator slack
or host transport buffers. Bound simultaneous host work and call lifetimes too.

Expiry retains revision fences. A lower revision fails even after expiry; an equal
revision must have identical canonical JSON content. A successful identical
revalidation can renew TTL. Replacement selects a fresh local epoch, preventing
delayed generation installation; reselecting an identical slot preserves its
revision fence. Restart starts empty. Independent slots are not installed as a
multi-field transaction; callers requiring atomic multi-field readiness must keep
that evidence unavailable.

## Transports and authorization

Registry production POST `/profile-snapshots` is disabled unless `PROFILE_READERS`
is configured as JSON `[{"tenant":"…","sha256":"64 lowercase hex characters"}]`.
Use a dedicated read secret, distinct from `PROFILE_PRODUCERS`. Configure it through
secret storage; never commit it. Bearer authentication precedes database lookup;
the authorized tenant must equal the requested tenant. Scope includes connection
and generation through the canonical tuple. The route queries one exact hourly or
daily rollup, validates its counters/bounds and returns 404 for missing history,
400 for malformed selection, 401/403 for denied credentials/scope and 503 for
unavailable/invalid data. Responses are private `no-store`. It uses registry SQL
history because the latest-hour DO cache cannot serve selected seasonal
slots. No second latest-snapshot/seasonality policy is introduced.

- Native: `refresh::native::HttpsReader` invokes installed curl over HTTPS, with
No redirect following. Token and request use private stdin config, never argv.
  Request cap is 8 KiB, response cap 128 KiB, with bounded connect/total time.
- WASM: [`wasm/refresh.ts`](./wasm/refresh.ts) exports `createSnapshotTransport`.
  A host with standard fetch/Request/AbortController supplies the endpoint,
Tenant and reader token. Send `Job::body` JSON with the job timeout; return
The bytes to the same Rust cache owner's `complete`. It bounds streams and
Cancels timeouts, refuses redirects, and requires no Worker binding. The host
Owns guest invocation/ABI and must honor fetch cancellation. No automatic WASM
Runtime scheduling or new guest export is implied.

TLS and configured credentials authenticate the registry. The provenance string
is an additional checked protocol discriminator, not a signature. Transport
errors must become `Error::Transport`; do not feed credential-bearing diagnostics
into guard/audit payloads. The local cache cannot authenticate arbitrary bytes
provided by a malicious host; the host is the credential/trust boundary.

## Telemetry and verification

`metrics` exposes attempts, successful installs, failures, rejected completions,
conflicts and missing/stale reads. `telemetry(id, now)` exposes refresh age, next due
time, in-flight state and failed attempts. Export these from background monitoring;
no diagnostic callback executes on a local read. Age measures time since successful
validation, not the age of a historical seasonal baseline. No latency claim
includes a remote request.

Local suites cover authenticated Worker/Neon boundaries in workerd, real PGlite
rollups, ingestion, WASM host transport, native curl controls, monotonic
scheduling, conflicts/expiry/reselection and original guarded error identity.
```bash
cargo test -p agw-stats --test refresh
cargo test -p agw-guard --test profile_refresh
cargo check -p agw-stats --target wasm32-unknown-unknown
pnpm --filter cell-registry exec vitest run tests/profile-refresh*.test.ts
pnpm --filter cell-relay exec vitest run --config wasm/vitest.config.ts
```
completed with actual relay exports from synthetic observations, native/WASM
cache validation and recovery. Temporary credentials/bridge access were revoked.
For a separately authorized repeat exercise, on the isolated staging
Worker, install a separate reader secret and use an authorized private HTTPS bridge
or service binding. Ingest traffic first, wait for Queue commit, then select
its exact scope/bucket/bounds. With endpoint, tenant and token set in the protected
`AGINDOW_PROFILE_ENDPOINT`, `AGINDOW_PROFILE_TENANT`, `AGINDOW_PROFILE_READER_TOKEN`
environment variables, run:
```bash
cargo run -p agw-stats --example refresh_snapshot < /private/tmp/selected-profile.json
```
Record sanitized revision, refresh lag, wrong-tenant denial, outage/expiry recovery
and selected-history retention in the.
Do not claim hosted delivery from local tests. No public route, credentials or
production rollout is enabled by this change.
