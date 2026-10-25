# Local Worker execution host (TI-34)

The real `workers-rs` handler implements explicit `POST /v1/guard` using the
same Rust two-phase statistical engine as the sidecar and Node guest. NullEngine
is the default and detects nothing. This source is deployment-ready configuration;
no live deployment or hosted latency qualification has been performed.

Install the pinned build tools and build from the repository root:

```sh
cargo install wasm-bindgen-cli --version 0.2.129 --locked
cargo install worker-build --version 0.7.5 --locked
pnpm --filter cell-relay run build:hosted
pnpm --filter cell-relay run test:hosted
pnpm --filter cell-relay exec wrangler deploy --dry-run --config hosted/wrangler.jsonc
```

`WASM_BINDGEN_BIN`, `ESBUILD_BIN`, and `AGINDOW_WORKER_BUILD` can name local
executables. worker 0.7.5 retains the repository Rust MSRV; compatibility date
2026-08-06 matches the pinned local workerd runtime. Tests execute the compiled
release Rust/WASM Worker, with an outbound stub and no vendor/network calls.

Supply `GUARD_KEYS` and `GUARD_CONNECTIONS` as local `hosted/.dev.vars.local` secrets (never
commit them). Keys contain the SHA-256 digest of the bearer token, tenant,
allowed `connection/tool` routes, plan label, `limit`, and `window_seconds`.
Connection bindings contain tenant, fixed HTTPS URL, HTTP method, operation,
optional bound headers and explicit local statistical selection. No envelope
can supply a credential or arbitrary upstream URL. Writes/unknown operations
require an upstream-enforced `idempotency_key`; the host never retries.
GET/HEAD bindings carry no body; bind read RPCs to POST when arguments are needed.

```sh
pnpm --filter cell-relay exec wrangler dev --env local --config hosted/wrangler.jsonc
curl http://localhost:8787/healthz
curl http://localhost:8787/v1/guard -H 'Authorization: Bearer local-example' \
  -H 'Content-Type: application/json' \
  --data '{"tool":"read","connection":"example","operation":"read","args":{},"mode":"observe"}'
```

Quota counters are bounded and **isolate-local**, not globally enforced billing
entitlements. `X-RateLimit-Limit/Remaining/Reset` and `X-Agindow-Quota-Scope`
make that explicit. Credentials use a constant-time digest comparison. Modes are
frozen per call. Cloudflare production [timers advance only after I/O](https://developers.cloudflare.com/workers/runtime-apis/performance/),
so inline CPU measurement is unavailable there. By default
`X-Agindow-Latency-Ms` and `X-Agindow-Latency-Clock` both say `unavailable`.
The explicitly local `--env local` configuration enables `workerd-local` active
intervals, excluding incoming upload, upstream and body-network waits. Never
deploy that local clock configuration. Provider post-request CPU metrics are
separate; client RTT is never reported as engine CPU time.

Policy halts return exact six-field 409 feedback. Request halts dispatch nothing.
Response halts retain original status/body at a same-origin `X-Agindow-Outcome`
handle and warn against replay; retrieval requires the same credential. Artifacts
and outcomes are bounded FIFO process-local state (64 records/16 MiB), lost on
isolate eviction. Unknown/oversized/compressed capture streams retain original
bytes and bypass unavailable inspection. SDK 5xx/transport fallback is TI-41.

The optional `/internal/registry-probe` administrative route exercises the
`REGISTRY_MTLS` Fetcher binding against a fixed `REGISTRY_HEALTH_URL`, protected
by `REGISTRY_PROBE_KEY`. Provision a real certificate and add its binding before
using it. Registry I/O is never a dependency of a guard decision. Live mTLS,
distributed quotas, durable replay, default selective repair, deployment and
production qualification remain pending; no source manifest supplies those claims.

Opt-in EPIC-14 machine identity adds local token/proof validation and a bounded
`/v1/token` exchange. Configuration and limitations: [machine identity](../MACHINE_AUTH.md).
