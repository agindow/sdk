# Self-hosted keyless execution owner

Opt-in profile; local evidence exists, live qualification is pending under
. The proposed execution
origin is `https://guard.agindow.com`, separate from dashboard cookies. The checked-in
Worker is disabled. This directory does not provision a public endpoint by itself.

## Admission and lifecycle

In manual mode (`wrangler.jsonc`), the private `KEYLESS_REGISTRATIONS` JSON has exactly `issuer`, `not_after` (Unix
seconds, at most one hour ahead), and `registrations` (1–256). Each row has exactly
`id`, `kid`, `x`, `audience`, `tenant`, `environment`, `connection`, `tools`.
`id` is the immutable client ID and assertion issuer/subject; `kid` and `x` are
the admitted Ed25519 public JWK key ID/coordinate. `audience` is the exact assertion
audience, conventionally the HTTPS `/v1/token` URL. Environment is `test` or `live`.
Tools are exact strings, without wildcards. No public registration endpoint exists.
Install reviewed policy with Wrangler secret stdin; never commit private operator
snapshots or workload private keys. Refresh the reviewed expiry out of band.

`GUARD_CONNECTIONS` uses the existing raw host's fixed connection/tool binding
format, including authoritative tenant/environment, upstream URL/method and
read/write operation. Only safe read bindings belong in initial qualification.
`EXECUTION_ORIGIN` and policy issuer must match exactly. `KEYLESS_ENABLED=true`
activates the profile after provisioning and review. No legacy bearer keys are
accepted. The owner generates its own transient signing keys; clients keep their
separate assertion private keys in workload secret storage.

The authority namespace is SHA-256 of `agindow-owner-v1:` plus immutable
registration ID. Scope edits never move a registration to a fresh namespace.
Changing the prefix, class, namespace or owner routing is a security migration;
retain the ledger and ownership identity, or provision a fresh namespace with its
full waiting period and invalidate prior credentials. Never route to both old
and replacement authorities concurrently. User-supplied owner hints only route;
the native host verifies signature, proof binding and scope before dispatch.

A new/empty owner waits **360 seconds** before token exchange. Accepted assertions
have at most 300s lifetime, 30s future-issuance skew and 30s expiry skew; durable SHA-256(registration + ':' + jti)
fences survive owner restart through assertion expiry+30s. Capacity is 1024 live
fences per owner, with no eviction to admit new assertions. Duplicate assertions
reject before minting. Native invalid assertions never create durable fences.
Tokens are exposed only after the fence commit succeeds.

Proof replay fences remain local to execution, with a fresh issuer epoch for each
owner activation **and** WASM reincarnation. Losing fences also loses the old
signing authority. Old tokens return 503, requiring SDK background refresh; allow
up to 335s recovery with a 300s token and a warm authority, or 370s after
empty-store quarantine (360s + 5s retry + 5s exchange). These are conditional
bounds: the authority must be available, assertion signing prompt and the SDK
scheduler running. A persistent partition, expired policy or suspended workload
has no finite recovery guarantee. Known replay/invalid auth rejects and does
not permit SDK fallback. Guard calls perform no SQL/IdP lookup. SQL is used during
owner initialization and assertion exchange only. This protects authorization
admission, not exactly-once vendor execution or distributed quota accounting.
Retained audits/outcomes remain transient and are not restored after restart.

## Local commands and evidence

Run from the repository root after dependency installation and the relay Node SDK
build. Worker build needs the pinned `worker-build` tool described in the parent
hosted build script.
```sh
pnpm --filter cell-relay build:hosted
pnpm --filter cell-relay check:owner
pnpm --filter cell-relay test:owner
pnpm --filter cell-relay test:owner:mutations
pnpm --filter cell-relay build:owner
```
`build:owner` packages with Wrangler's dry run; it does not deploy or enable.
Bindings are generated with the project's Wrangler `types` command and
`--strict-vars=false`, rather than edited by hand. The local owner fixture uses
pinned Miniflare 5 and runtime date 2026-10-26; the new deployment configuration
uses 2026-10-27. Their one-day difference remains part of the live release gate.

The fixture creates three independent ingress/authority workers, persistent
SQLite, two independent native WASM incarnations, a fake clock and an upstream
dispatch counter. It exercises duplicate guard/delegation/GET proofs, assertion
races, persisted restart, verifier reset, lost storage, capacity, rollback,
malformed input and byte identity after guard SQL tables are removed. Fault RPCs
and clock/reset ports are fixture-only; production exports no such methods.

The public contains
a Node helper tested against the compiled raw host and Python signer guidance.
Local fault fixtures additionally cover ingress authority partitions, stale owner
routing, dropped exchange acknowledgements, policy refresh, expiry/skew retention,
per-tenant proof-cache saturation and controlled SDK recovery after both warm
restart and lost storage. Native caches partition by trusted owner activation:
1024 fences per workload, bounded to 256 retained activations per isolate;
activation saturation rejects without evicting replay state.

`owner_admission` logs contain only closed outcome counters and elapsed milliseconds;
`owner_ingress` counts authority RPC failures. Counters are activation-local, with
no credential, tenant, owner, URL or exception text. Known guard/assertion replay,
capacity and unavailability are counted separately. Retained GETs preserve their
opaque 404 rejection contract and count as rejected rather than disclosing why.
Upstream errors remain completed calls and their bodies are never inspected by
telemetry. Host log sampling/retention remains an operator responsibility.

`owner-faults.test.mjs` reports local direct-owner and routed p50/p95 milliseconds
separately; these include RPC, guard and synthetic upstream time, not just JWT
verification. Actual Cloudflare partition/rebalance, cross-region routing latency
and real Node/Python HTTPS qualification remain gates.
Do not remove the broader standalone Worker/sidecar process-local limitations.

Local measurement (October 27, 2026; Apple M2, Darwin arm64, Node 22.22.2):
direct-owner p50/p95 2.880/4.367ms; routed p50/p95 2.660/4.658ms over 20 samples
after four warmups. The -0.220ms median difference reflects harness noise, not a
routing speedup or a resolved pure routing increment. Separately, native release
JWT+DPoP mean/p99 is 95.24/240.00 microseconds over 1000 calls; native availability
upper bounds still pass the frozen <10 microsecond gate. See for commands
and verification coverage; none of these timings qualifies live traffic.

## Automatic renewal

[The private renewal runbook](control/README.md) describes the opt-in reviewed
store, offline operator commands, renewal-only job, rotation and emergency stop.
`auto.wrangler.jsonc` selects auto mode for the same execution Worker/DO namespace;
it needs no `KEYLESS_REGISTRATIONS` secret. A stopped, acknowledged migration is
required. Runtime rollback to manual mode refuses owners that have adopted auto
policy. Keep existing assertion fences and owner identity during that transition.

The job renews unchanged reviewed rows every ten minutes. Each admission lease
remains <=1h; an explicitly reviewed renewal mandate lasts at most 30 days.
Delivery alarms retry only delivery and cannot replace a stopped renewal job.
Expired leases refuse exchange; already-issued tokens continue under their local
TTL/epoch/scope checks without renewal I/O. Signed stop/revoke prevents further
renewal; stale delivered leases can retain issuance authority for up to 3630s,
followed by at most 330s token validity (conservative 66 minutes). Execution
shutdown via `KEYLESS_ENABLED=false` is a separate action with deployment
propagation that must be verified.

Only local compiled fixtures qualify these behaviors. The control/job configs
are disabled and private; live transport, resources and qualification remain
.
