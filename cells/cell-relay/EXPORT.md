# Relay profile export
The explicit local pipeline implements [contract §10.22](/agindow-runtime-contract.md).
It does not enable collection or networking on the default guard. The native
background sender supports Unix hosts with `curl` installed. Local staging and
projection also compile for WASM; a WASM background transport is not supplied.
Subsequently qualified the authenticated boundary in isolated staging
with actual relay exports from synthetic observations. Temporary access was
revoked; no continuous collector is enabled.

## Ownership and lifecycle

Configure the `Store` sample/scope/byte limits and the `Staging` window/byte
limits. After complete observations have been admitted, call
`Staging::close(store, scope, close_id, start, end)` at an explicit boundary.
Intervals are UTC milliseconds, half open, and must enclose every observation.
Each logical close ID is caller-owned and unique. Closing detaches an immutable
window and resets only that scope's live observations; it never publishes a
baseline. Frozen snapshots and other scopes remain valid. Baseline windows
must be explicitly published through separately.

Closing does no hashing, profiler work, filesystem access, Queue send or network
I/O. Staging rejects new closes on capacity pressure, increments `rejected`
and leaves the source window untouched. If the source fills too, admission
rejects new samples atomically. Monitor both error paths. Do not wait for an
exporter lock or channel from a guarded call: use exclusive local collection
and a bounded nonblocking handoff to the background owner.

On the background owner, `Outbox::collect_one(staging, cohort)` projects one
window, commits its exact envelope, then removes the staging front. A full spool
or disk failure retains the front and returns an error; projection rejection
records a durable loss and removes that invalid front. Sample timestamps remain
local; only the explicit enclosing observation interval is uploaded.

`Outbox::open(path, limits)` loads a bounded durable spool. Use one owner per
unique path, an operator-controlled directory and the same limits after restart.
The spool uses a temporary file, fsync, rename, and directory fsync; it is not a
multi-process queue. Unix files are created with mode 0600. Do not recreate the
outbox on disk failure: retain its in-memory state and retry. Uncommitted local
staging can be lost on process failure; committed envelopes replay exactly.

Pump only from the background lifecycle, for example:
```rust,ignore
// Background owner only; these handles never enter guard evaluation.
let mut outbox = Outbox::open(spool_path, limits)?;
outbox.collect_one(&mut staging, "small")?;
outbox.pump(utc_epoch_ms, |body| match sender.send(body) {
    Ok(accepted) => accepted,
    Err(error) => { report_transport_error(error); false }
})?;
```
Each pump sends at most one profile. Attempt reservations and exponential
backoff deadlines persist before sending. Backoff caps at one hour, configuration
permits at most 100 attempts, and exhausted entries become durable `losses`.
Capacity refusals increment durable `rejected` rather than claiming actual
loss. Admission reserves worst-case retry metadata and counter growth before accepting
a profile. Limits cover serialized spool bytes (at most 16 MiB), pending windows
(at most 4096), attempts and deadlines; one window is at most 4096 samples and
64 KiB string input. Include counters/errors in the host's existing telemetry;
there is no metrics server or implicit background thread. Redelivery after a
lost acknowledgement is intentional: 's immutable insert is idempotent.

## Scope and numeric projection

No envelope version changes. `tenant_segment` is the exact canonical JSON tuple
`["ti48", connection, generation, segment, window]`. Every other scope dimension
maps directly and `volume_cohort` is explicit trusted configuration. Overlong
identities fail v1 bounds instead of truncating or sharing independent baselines.
Keep the same cohort for a logical window. Window IDs hash scope, close ID,
observation timestamps and summary. No raw values are sent.

Missing, null, boolean, string and numeric type slots remain separate. Only
the `ExactInteger(Observed)` values within ±(2^53−1) enter numeric bins.
Decimals, exponents, disabled/high-precision projection and unsafe integers
increase `excluded_numbers`; no floating parse or rounding occurs. Integer
lexemes occupy the integer slot; decimal/exponent lexemes occupy the number slot.
Exact numeric lexemes feed the categorical hash, so lexically distinct numbers
remain distinct evidence. Hashes provide coarse summaries, not anonymization.

## Authenticated hosted boundary

`HttpsSender::new(endpoint, tenant, token, timeout_ms)` permits HTTPS and a
1–30000 ms deadline, rejects header injection, sends credentials through curl's
stdin rather than process arguments, disables curl config files/globbing and
follows no redirects. Only HTTP 202 acknowledges the local spool. A sender is
bound to one trusted tenant and refuses other tenant bodies before starting curl.
A 202 means Queue acceptance; the consumer acknowledges after persistence/cache
publication and retries to the existing bounded DLQ on failure.

The registry Worker implements `POST /profiles`, disabled unless the optional
`PROFILE_PRODUCERS` secret is configured. Its secret value is JSON:
```json
[{"tenant":"operator-owned-tenant","sha256":"64 lowercase hex characters"}]
```
Use a unique random bearer credential of at least 32 bytes per producer tenant;
store its SHA-256 digest in this map (maximum 256 entries). Provision the actual
token only on that tenant's native host. Never accept a tenant identifier from a
request as authorization. The receiver authenticates the token, validates at
most 64 KiB of streamed body, compares its tenant with the trusted map and awaits
Queue acceptance before returning 202. Missing credentials/configuration, tenant
mismatches and Queue failures cannot acknowledge delivery. Snapshot reads
use a separate tenant-bound reader credential and `/profile-snapshots` route.

Deployment requires the operator to supply this secret and an intended HTTPS
route while retaining existing Queue/DO/database bindings. Default configuration
keeps public routes disabled. Follow the
for isolated staging qualification;
owns remote snapshot loading.
records completed isolated staging qualification and access revocation.

## Verification

`cargo test -p agw-stats --test export --test export_native` covers local admission,
projection, scope mapping, byte/count pressure, disk faults and retry restart.
The transport's unit test checks credential handling and acknowledgement codes.
The guard integration test checks original byte/outcome identity during staging
pressure and cold-engine bypass. Registry tests validate Rust-produced envelopes
through authenticated ingress and real SQL, duplicate/reordered delivery,
consumer recreation, and the production Worker/Queue binding in workerd.

Regenerate the shared test fixture after intentional projection changes:
```bash
cargo run -p agw-stats --example export_profiles > cells/cell-relay/crates/stats/tests/support/relay-profile.json
```
Rust asserts exact equality against that fixture; TypeScript validates it with
the actual frozen schema and counter checks. Neither test calls a remote service.
