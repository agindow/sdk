# Credential evidence and cutover

Local, unpublished library surfaces implementing
And
[contract §10.53](/agindow-runtime-contract.md). Local qualification results and remaining review limits are recorded in
. No provider is
contacted by these libraries and no credential is minted, revoked or read.

## Response evidence

`agw_guard::credential::session::Session` belongs to one tenant/connection binding.
The host supplies distinct outcome/rotation event IDs, bounded operation/argument
identity tokens, a monotonically
injected clock, a UTC timestamp, audit/event identifiers and optional credential
version. Never pass argument bodies or Authorization as tokens. Host references
are not credential hashes. `Session::forward` moves the original upstream
response into the existing streaming forwarder; status and bytes survive.
`Session::observe` returns at most two events (rotation plus outcome), separately
from the caller's response. The host drains these outside the request path.
Bypasses and incomplete evidence produce no events and never advance history.
Credential evidence is capped at 128 headers, 4096 bytes per auth challenge,
128 bytes per quota marker and 4096 JSON body bytes. Original forwarding is
independent of those inspection bounds.

Only declared invalid/expired-token directives provide provider authority.
Unmarked 401 is rejected with unknown evidence. Quota markers do not diagnose a
dead credential. A 403 scope inference requires prior success of the exact
operation and host argument identity under the same version. Success after
credential failure emits one recovery; versions are never inferred from secrets.

`agw_audit::credential_wire::parse` enforces the closed event and cross-field
rules; the generated Rust structural type alone cannot enforce conditional
schema relationships. TypeScript uses `parseCredentialEvent`. All cross-cell
consumers must use these validation boundaries. Known credential-shaped strings
are rejected. Arbitrary opaque host strings are inherently indistinguishable
from arbitrary secrets, so host identifier provenance remains necessary.

## Health

`Projection` is constructed for exactly one authenticated tenant. Its bounded
records use host-assigned per-binding sequence numbers to reject stale/repeated
deliveries. Records carry independent health and last cause, first/last failure,
last success and revision. Capacity eviction is oldest-first with a drop count.
Expiry is an explicit host input and emits a one-time local signal. Successful response observations can update liveness with `success` without
inventing credential events. There is no
wall-clock read or background expiry task. `apply_window` canonicalizes a bounded shuffled timestamp window in place;
single-event delivery requires host ordering. Reconstruction across arbitrary
previously finalized windows is not provided.

## Operator cutover

`Cutover<B>` contains `Arc<B>` opaque host binding handles. Freeze a bounded
probe plan with `set_plan`, then `start`. `candidate` validates admission before
returning a candidate handle. The host executes that probe directly, outside
statistical windows, recovery counters and corpus collection, and supplies its
bounded result to `record_probe`. Receipts carry `canary: true`; do not route
probes through production statistical sessions. Read probes are the default;
writes/unknown operations require per-probe consent and an idempotency key
carried by the host. The library retains only key presence.

All planned operations must pass before `promote`. Production calls capture
`dispatch` once and execute it once. A handle already in flight survives
revert without replay. Quota consumes the same bounded probe budget and may be
retried. Faults and deadlines abort without touching production selection.
Unknown or atomic invalidation capability refuses start.

Promotion starts a bounded overlap. The host feeds candidate failures and
successes into `failure`; terminal expiry/revocation or the configured failure
threshold reverts selection and quarantines that cutover ID. The host drives
`tick`; the first clean expiry returns a release signal, subsequent ticks do
not. Persist `snapshot` before releasing the old binding. External release
idempotency is the host's responsibility. Restore requires matching binding
references and valid state; missing/corrupt state freezes to active/unknown.
Restore recomputes completed probes rather than trusting saved completion flags,
and checks counts and lifecycle/window coherence. The host protects snapshot
integrity and restores the matching bindings; the local snapshot is not signed.
There is no timer, thread, secret persistence or distributed coordination.
Unsupported receipts distinguish atomic invalidation from unknown capability.
Inconclusive non-credential HTTP probe results consume the bounded retry budget;
only a complete successful plan promotes. Credential failure, execution fault,
timeout, explicit abort or unsuccessful budget exhaustion aborts verification.

## Offline readiness CLI
```sh
agindow rotation-readiness events.ndjson facts.ndjson \
  --as-of 2026-10-22T12:00:00Z --json --require=verifiable
```
Both inputs are NDJSON. Events use `credential_event` v1. Each capability fact
has `tenant`, `connection`, `observed_at` (UTC Z), `source`, `dual_key`
(`yes|no|unknown`), `mint` (`manual|api|none|unknown`), and `replacement`
(boolean). Optional `issued_at` is host-attested; optional `scopes` is an array
of declared scope labels. Facts are an explicit offline interchange, not a
claim that research has been performed. Operations are shown separately
from declared scopes; no operation-to-scope mapping is inferred.

JSON schema identifier: `agindow.rotation-readiness/1`. Output has `as_of` and
stable ordered `rows`. Each row has `tenant`, `connection`, `verdict`, `reason`,
`health`, `last_ok`, `observed_since`, `observed_operations`, `auth_breadth`,
`mint`, `dual_key`, `sources`, and per-fact `provenance`. `issued_at` and the
atomic-invalidation `warning` are optional. Missing/stale/disputed facts yield
unknown. Facts expire after 30 days by default. Manual mint with dual keys is
verifiable. `observed_since` is explicitly a lower bound, never credential age. Facts-only
connections have null observation timestamps and unknown health. Report health
follows host-declared version rotations and ignores late superseded-version
outcomes. A mixed-version recent window without rotation authority is unknown.
Equal timestamps use host export order; quota is excluded from health history.

Memory is bounded by 4096 connections, 256 operations per connection, 16 facts
per connection and 8192 bytes per input line. Events are streamed; the entire
event file is never loaded. Exceeding a bound or malformed input exits nonzero
with a bounded error and no partial table. `--require=verifiable` exits nonzero
if any row fails that verdict. No network, bindings, secrets or file mutation
are involved.

## Reproducible checks
```sh
node cells/cell-relay/node/scripts/verify-rotation.mjs
cargo test -p agw-guard --test credential --test credential_bounds --test credential_health \
  --test credential_cutover --test credential_restart --test credential_session
cargo test -p agw-audit --test credential_wire
cargo bench -p agw-guard --bench credentials
```
The offline script transpiles only the report and contracts in a temporary
folder, checks deterministic output and exit codes, and streams 1,000,000 distinct events
under a 64 MiB Node heap. It does not build or require the WASM guest. See the linked evidence for measured native benchmarks and the final gate result.

Registry consumers can use `CredentialIngress` for authenticated, closed-schema
admission with a bounded tenant-local identity ledger. Duplicate IDs with equal
content are no-ops; conflicting content is rejected. At capacity, admission stops
and a drop counter increments rather than silently forgetting an admitted ID.
Persistence and restoration of that ledger remain host responsibilities.
