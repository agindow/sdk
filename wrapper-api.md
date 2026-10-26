# 🧩 AGINDOW — Shared Wrapper API
**The one API every integration targets** · Drafted Oct 17, 2026 · Parent:

> **This is the load-bearing interface.** Pipedream, LangChain, Mastra, and Nango all adapt to it. If the API is wrong, every integration is wrong — so we prove it on two platforms before touching Nango.
>
> **Design rule: the API is about *policy*, not *transport*.** Anything that differs between platforms belongs in an adapter. Anything here must be identical everywhere.

---

## 1. The five requirements this API has to satisfy

| # | Requirement | Source | How the API delivers it |
|---|---|---|---|
| 1 | **Zero-friction install** — one line, no rewrite | GTM §4 | `agindow.wrap` — infers the adapter |
| 2 | **Mode is a runtime parameter, never a migration** | contract §2 | `mode` on the config object, hot-reloadable |
| 3 | **Never take down the agent** | contract §4.1 | Errors are caught *before* they can escape. Async engine load, sync API. |
| 4 | **Byte-identity preserved** | contract §3 | The wrapper **forwards the original return value by reference**. It never reserializes. |
| 5 | **Platform-neutral** | | Adapters only; zero platform concepts in core |

### The two hard problems, and how they're solved

**Problem 1 — synchronous API, asynchronous engine.** The Rust/WASM engine loads asynchronously. But `agindow.wrap(tools)` must return instantly and the app must work immediately.

> **Solution: the engine loads in the background; calls before it's ready are forwarded untouched.** The first few calls are unprotected, and we *say so* rather than pretending otherwise. `agindow.ready` exposes the promise; `mode: 'strict'` waits for it.

This is a genuine limitation and it should be visible: `X-Agindow-Status: warming` during that window. It also means **the OSS shim's first run is unprotected**, which is an honest thing for a security-adjacent product to admit.

**Problem 2 — byte-identity vs. Usefulness.** If we return the *original object by reference*, byte-identity is trivially preserved and latency is near-zero. The cost: on the repair path we must construct a new object.

> **Solution: return-by-reference on the no-repair path, new object only on repair.** Contract §3.1's `byte_identical: false` is then not a caveat — it's a *fact we compute*, and it's `true` by default.

---

## 2. The public API surface
```ts
import { agindow } from 'agindow'

// ── The universal entry point ──────────────────────────────────
const tools = agindow.wrap(existingTools, {
  mode: process.env.AGINDOW_MODE ?? 'observe',
})

// ── Or be explicit ─────────────────────────────────────────────
import { toolAdapter, clientAdapter, httpAdapter } from 'agindow'

const tools  = toolAdapter(existingTools, config)    // LangChain, Mastra, AI SDK
const client = clientAdapter(nangoClient, config)    // Nango, Composio, fetch
const app    = httpAdapter(expressApp, config)       // any HTTP stack
```
Three surfaces, one policy model. Everything else is an adapter.

### Full surface

| Export | Purpose |
|---|---|
| `agindow.wrap(target, config?)` | **Auto-detect** the adapter. The one-liner. |
| `toolAdapter(tools, config)` | Wrap a tool map / tool array (LangChain, Mastra, AI SDK) |
| `clientAdapter(client, config)` | Wrap an API/proxy client (Nango, Composio) |
| `httpAdapter(app, config)` | Wrap an HTTP app or a single `fetch` |
| `agindow.guard(fn, config)` | Guard one function — the primitive everything builds on |
| `agindow.ready` | `Promise<void>` — resolves when the engine is loaded |
| `agindow.configure(config)` | Hot-reload config at runtime |
| `agindow.on(event, handler)` | `drift` · `repair` · `blocked` · `bypass` · `error` |
| `agindow.simulate(tool, payload)` | **Dry-run against a recorded payload.** No upstream call. |

**Inference is settled (§12.1): `wrap` auto-detects, and a target matching
more than one adapter throws a named configuration error** listing the
available adapters and the explicit call for each — never a silent guess.
Explicit adapters remain first-class. Setup-time errors throw before any call
runs; runtime faults never throw (§6, contract §4.1).

**This surface is normative across languages (§12.4).** The Python SDK mirrors
the primitive semantics exactly, enforced by a generated cross-language
contract suite over shared fixtures that fails CI on divergence; any
unavoidable difference is an explicit, reviewed exception.

---

## 3. Configuration
```ts
interface AgindowConfig {
  // ── Identity ──────────────────────────────────────────────
  apiKey?: string                    // agw_live_…; absent → local-only mode
  endpoint?: string                  // default: managed cloud

  // ── Policy (contract §2) ──────────────────────────────────
  mode?: 'observe' | 'repair' | 'strict'
  failClosed?: boolean               // default true. Writes protected. §4.1
  allowedTools?: string[] | '*'       // scope the blast radius
  denyTools?: string[]

  // ── Deployment ────────────────────────────────────────────
  sidecar?: string                   // sidecar URL; enables zero-code mode
  sync?: { contracts?: number; drift?: number }   // poll intervals, seconds

  // ── Observability ─────────────────────────────────────────
  onEvent?: (e: AgindowEvent) => void
  buffer?: number                    // ring buffer size; 0 = disabled
  spans?: boolean                    // OTel span per guarded call; default true
  debug?: boolean                    // stderr logging

  // ── Escape hatch ──────────────────────────────────────────
  headers?: Record<string, string>    // passthrough metadata
  transformers?: Record<string, Transformer>   // vendor-specific overrides
}
```
### Two decisions worth defending

**`mode` lives on the config, not per call.** Per-call mode would invite `'strict'` in a hot path and 'observe' in a test — drift between environments, exactly the class of bug we're selling against. Per-call *override* is allowed but defaults to the config value.

**`allowedTools` defaults to `'*'`.** Narrowing on install would be safer, but it'd be friction on the one-liner and would hide the product's value behind a config wall. Ship open; let the demo show the blast radius; let ICP-3 narrow it during security review — which is the conversation we *want* to have.

**Config resolution is ordered (§12.3):** explicit `wrap`/`guard` config →
environment (`AGINDOW_MODE`, …) → `.agindowrc` → defaults. The file (JSON,
nearest to the working directory) lets production config live in version
control; the environment overrides it per deployment. An unreadable file warns
loudly and is ignored — config never breaks startup.

### native opt-out delivery

`failClosed` remains the public SDK option designed in §3. The native reference
implementation supplies attributed administrative controls, warning/audit
receipts and frozen per-call protection in both explicit guards (runtime
contract §10.36). Supplies a local standing banner that subscribes
to host-adapted control receipts and cumulative per-epoch write-drift counts.
These are library APIs, not an implemented SDK configuration transport or hosted
dashboard. Semantic repair writes and strict escalation cannot be overridden.

### native replay delivery

The native `gated::Call::execute_once` boundary requires write/unknown keys,
returns deterministic 400 admission errors for invalid envelopes, and preserves
original outcomes and terminal feedback across replay (runtime contract §10.37).
A tenant-local store serializes identical keys and supplies immutable upstream
key/durability bindings. SDK and HTTP owners must explicitly integrate this
boundary with bounded owned response values; no SDK transport or automatic retry
policy is implemented. Expiry, process loss and independent hosts require upstream
or durable coordination. Local replay protection alone never proves write integrity.

### native review delivery

Strict blocked calls can export bounded `review_case` v1 evidence off-path;
Replay retains that same evidence. Local console controls produce attributed
`review_resolution` v1 receipts and stable per-connection SLA breach alerts.
Registry feedback is descriptive and capped at 0.989. Hosts own authentication,
persistence, delivery and polling. Human decisions cannot bypass strict policy,
execute a write or release terminal keys. See runtime contract §10.38 and the
[native review API](/cells/cell-relay/REVIEW.md).

---

## 4. The primitive — `agindow.guard`

Every adapter reduces to this. **If it's not expressible here, the adapter is wrong.**
```ts
agindow.guard(async (args: Args) => {
  return await upstream.call(args)      // returns anything
}, {
  tool: 'stripe.create_customer',        // registry identity
  operation: 'write',                    // drives §4 policy
  toolName?: (args) => string,           // for dynamic endpoints
  inputShape?: 'raw' | 'parsed',         // raw = don't pre-parse
})
``````ts
function guard<A, R>(fn, opts): (...args: A[]) => Promise<R>
```
### The contract

1. **Return-by-reference.** On the no-repair path, resolve with `fn`'s exact value. Never clone, never reserialize, never `JSON.parse`/`stringify`.
2. **Errors pass through untouched.** If `fn` throws, `guard` rethrows the same error — **unless** it's drift-classified, in which case it throws the enriched `AgindowDriftError`.
3. **Never throws its own errors.** Engine faults resolve with the original value plus `X-Agindow-Status: bypassed`.
4. **Async transparent.** Never converts a sync fn to async or vice versa.
5. **Argument arrays preserved.** `guard` forwards all args; only the ones `opts` names are inspected.
6. **`this` is preserved.** Methods keep their receiver.

### Why #1 and #2 are the whole game

- **#1 is the byte-identity guarantee.** A wrapper that re-serializes would silently corrupt the customer's payloads — the exact bug class contract §3.1 exists to prevent. Returning by reference makes it structurally impossible.
- **#2 is the trust guarantee.** A wrapper that swallows or rewrites application errors is a debugging nightmare. Drift classification is *additive* — the original error survives in `.cause`.

---

## 5. Adapters

### 5.1 `toolAdapter` — LangChain, Mastra, AI SDK

The most common shape: an object of tool definitions.
```ts
// Input shapes we accept
type ToolMap   = Record<string, ToolFn>
type ToolArray = readonly ToolDef[]
``````ts
const wrapped = toolAdapter({
  searchWeb: async (q: string) => { /* … */ },
  bookFlight: async (flight: FlightArgs) => { /* … */ },
}, { mode: 'repair' })
``````ts
// LangChain — preserves .name, .description, .schema, .func
const lc = toolAdapter(langchainTools, config)

// Mastra — preserves execute/id/description
const mastra = toolAdapter(mastraTools, config)

// AI SDK — preserves execute + zod schema
const ai = toolAdapter(aiSdkTools, config)
```
**What must survive the wrap — non-negotiable:**

| Property | Why |
|---|---|
| `name` | Model-facing identity. Breaking it breaks tool selection. |
| `description` | Same |
| `schema` / `zodSchema` / `inputSchema` | Validation. If the model can't see the schema it can't call the tool. |
| `func` / `execute` | The implementation |
| `returnsSingleValue` | Framework-specific behavioral flag |

**Test for this:** a property test asserting that `Object.keys` and every preserved property are **identical** before and after wrapping. A wrapper that drops `description` is worse than no wrapper — the model silently stops calling the tool.

**Injection method:** where the framework lets us hook the executor (LangChain's `ToolCall` path, Mastra's middleware), we intercept there. Where it doesn't, we wrap the function and re-attach metadata. **Never monkey-patch the framework itself.**

### 5.2 `clientAdapter` — Nango, Composio, fetch

Wraps an object with HTTP-ish methods.
```ts
const nango = clientAdapter(new Nango({ secretKey }), {
  mode: 'repair',
  tool: (req) => `nango/${req.endpoint}`,   // dynamic tool identity
})
```
**Method discovery:** walk the client's own enumerable properties (and prototype), wrap anything matching `get|post|put|patch|delete|request|fetch|proxy`. **Skip everything else** — `auth`, `connect`, `disconnect`, `batchSave`, `getCheckpoint` are not HTTP calls and wrapping them would be wrong.

**Critical: never wrap `batchSave`/`getCheckpoint`/`saveCheckpoint`.** Those write to *Nango's* storage, not a third-party API. Wrapping them means inventing drift semantics for data that never crossed an API boundary. **Skip-list is explicit, not heuristic.**

### 5.3 `httpAdapter` — any HTTP stack
```ts
const app = httpAdapter(expressApp, config)      // whole app
const one = httpAdapter(fetch, config)            // global fetch
```
**Response handling:** this is the only adapter that sees raw bytes, so it's the only one that can truly guarantee byte-identity. Buffer, tee to the inspector, forward the **original buffer** on the no-repair path.

**Streaming:** if `Content-Type` isn't JSON, or the body is chunked, **stream straight through** and skip inspection. Inspecting a stream would consume it.

### 5.4 Zero-code mode — no adapter needed
```bash
AGINDOW_KEY=agw_live_… AGINDOW_MODE=observe ./agent
AGW_PROXY=http://localhost:8787 npx some-agent --base-url …
```
The sidecar is the universal fallback. **It's the only surface that works everywhere**, which makes it the answer for ICP-3 and for any platform we can't ship a package for.

---

## 6. Error model

### `AgindowDriftError` — what the agent sees
```ts
class AgindowDriftError extends Error {
  readonly name = 'AgindowDriftError'
  readonly code: 'drift_blocked' | 'drift_halted' | 'contract_violation'
  readonly tool: string
  readonly driftClass: 'shape' | 'semantic' | 'unknown'
  readonly driftId: string
  readonly hint: string              // actionable, model-readable
  readonly auditUrl: string
  readonly cause?: unknown           // the original error, preserved
}
```
**The `hint` is the product.** Research on this is unambiguous: structured feedback drives recovery where unstructured errors cause blind retries. Write hints as instructions, not diagnostics:
```
✅ "Vendor renamed `source` → `payment_method` on 2026-10-17.
    Re-issue using `payment_method`. Do not retry `source`."
❌ "Schema drift detected at path data.source. Severity: breaking."
```
Second sentence tells the model nothing it can act on. First one ends the retry loop.

### `AgindowBypassError` — never thrown

Bypasses are **never errors**. They're normal operation. The call returns its original value with a `bypassed` marker, and the event goes to `agindow.on('bypass', …)`. Throwing here would make us an outage — contract §4.1.

### Install-time failures — degrade, never fatal

An adapter install failure is not a runtime error: a tool entry that can't be
wrapped, or an adapter that throws while attaching, is unwrapped and warned
about, and the app always starts (§12.5). When the failure leaves nothing to
fall back to — an ambiguous `wrap` target, invalid config — it is a named
setup error thrown before any call runs instead (§12.1).

---

## 7. Events & observability
```ts
agindow.on('drift',   (e) => { /* detected */ })
agindow.on('repair',  (e) => { /* applied, with confidence */ })
agindow.on('blocked', (e) => { /* write refused */ })
agindow.on('bypass',  (e) => { /* engine unavailable, passed through */ })
agindow.on('error',   (e) => { /* our fault, not theirs */ })
```
Every event carries `{ tool, mode, driftClass?, side?, confidence?, latencyUs, auditId? }`. **OTel spans are emitted automatically** — `agindow.guard` opens a span, closes it with the drift attributes. No manual instrumentation required. Spans default on (§12.2) and open through the OTel API, so the application's SDK governs sampling and export; `spans: false` disables them, and with no SDK registered span creation is a no-op.

Events are delivered synchronously to listeners and retained only in memory.
There is no buffered event transport or `agindow.flush` shutdown hook. Hosts
own asynchronous listener work and exporter shutdown. Opt-in guest audit
publication uses the separate explicit `drainAudit` API (§12.10). This corrects
the unimplemented §2 entry ( completion audit, October 22, 2026).

**Ring buffer:** the last N events are retained in memory, readable via `agindow.recent`. Zero-infrastructure debugging — and the thing that makes the demo reproducible.

---

## 8. `agindow.simulate` — the sales and testing primitive
```ts
const r = await agindow.simulate('stripe.create_customer', {
  recorded: await loadFixture('stripe-2026-10-17'),
  args: { email: 'a@b.com', source: 'tok_123' },
  mode: 'repair',
})
// → { status: 'repaired', confidence: 0.97, changes: [...], wouldWrite: true }
```
**No upstream call. No credentials. No production risk.** Takes a recorded payload and answers: *what would you have done?*

Three uses, in ascending order of value:
1. **CI** — assert behavior on recorded drift fixtures
2. **Internal eval** — this is what 's benchmark harness runs on
3. **Sales** — a prospect pastes their own API shape and sees the verdict before installing anything

That third one is the most valuable and the least obvious. **The trial-without-install removes the entire security objection** — nothing touches their infrastructure.

---

## 9. `agindow.report` — the CLI that sells
```bash
$ agindow report ./src

  Tools detected        14
  Already wrapped        0  ⚠️
  Vendors                3  (stripe, shopify, salesforce)
  Known drift events    3  (last 30 days, from registry)
  Dry-run verdict:
    stripe/create_customer    → would repair 1 rename, confidence 0.97
    shopify/list_orders       → would BLOCK (semantic: enum expanded)
    salesforce/query          → no drift observed

  Install:  npm i agindow && agindow.wrap(tools)
```
This runs offline against code + public registry data. **No API key, no signup, no telemetry.** A prospect gets a specific, quantified answer about their own codebase in under a minute — and we made the argument better than we could.

> This is the highest-leverage artifact in the integration story. It costs a CLI to build and it's worth more than the integrations it precedes.

---

## 10. Adapter compatibility matrix

| Platform | Adapter | Preserves | Constraint |
|---|---|---|---|
| **Pipedream Connect** | `clientAdapter` | ✅ full npm | None — full WASM |
| **LangChain / LangGraph** | `toolAdapter` | ✅ name/description/schema | None |
| **Mastra** | `toolAdapter` | ✅ name/description/zod | `ee/` license — verify vendoring |
| **AI SDK** | `toolAdapter` | ✅ execute/zod | Uses built-in `repairToolCall` — we sit *outside* it |
| **Nango** | `clientAdapter` + `toolAdapter` | ⚠️ partial | **No npm in sandbox** → needs zero-dep codegen for `exec` bodies |
| **Composio / Merge / Paragon** | ⚠️ MCP only | ❌ | Observe-only. SDK unreachable. |
| **Any HTTP stack** | `httpAdapter` | ✅ raw bytes | Full byte-identity |
| **Sidecar** | none needed | ✅ | Works everywhere; the universal fallback |

---

## 11. What this API deliberately does NOT do

| Not doing | Why |
|---|---|
| Per-call `mode` as the primary control | Environment drift. Config is the source of truth; per-call is an override only. |
| Retrying failed calls | Contract §5 idempotency is the engine's job, not the wrapper's. |
| Caching responses | Changes semantics invisibly. Caching is the app's decision. |
| Auto-installing or self-modifying code | We wrap; we don't rewrite. The caller owns their source. |
| Swallowing application errors | Debugging nightmares. Drift classification is additive; `.cause` is preserved. |
| Depending on any framework | Core has zero framework imports. Adapters live in separate packages. |

---

## 12. Open questions — decisions

*Originally 5 open questions. All 5 decided Oct 18, 2026; the settled
behaviour is recorded in the body sections above. New questions stay open
here until decided.*

### ✅ 12.1 `agindow.wrap` auto-detection — DECIDED

**Decision: auto-detect; a named configuration error on ambiguity.**

Targets matching exactly one adapter shape resolve without ceremony — a tool
map/array, a client, an HTTP surface. A target matching more than one adapter
throws synchronously from `wrap` with the available adapters and their
explicit call forms; it never guesses.

Rationale: a silent wrong wrap is worse than a failed boot — the developer is
present at setup and the failure is deterministic and actionable. This is setup
misuse, not an engine fault, so the never-throw rules (§6, contract §4.1)
govern calls, not installation. The original *"revisit after Pipedream +
LangChain"* condition now governs implementation timing only: lands
after /.

### ✅ 12.2 Default OTel spans — DECIDED

**Decision: spans open by default, sampled by the application; `spans: false` disables.**

A span per guarded call is free observability where an OTel SDK is present and
a no-op where it isn't. The wrapper opens spans through the OTel API and leaves
sampling, batching and export to the application's SDK — the standard
library-instrumentation division of responsibility — so "sampled" needs no
wrapper-side policy.

### ✅ 12.3 `.agindowrc` config file — DECIDED

**Decision: JSON file, loaded nearest to the working directory; precedence explicit > environment > file > defaults.**

Production config belongs in version control; deployments override via the
environment; tests override via the explicit argument. An unreadable or
invalid file warns loudly and is ignored — config never breaks startup. Lands
with 's config resolution (§3).

### ✅ 12.4 Python parity — DECIDED

**Decision: the TS surface is normative; generate the cross-language contract suite.**

The primitive API must be identical in both languages or the SDKs drift. A
generated suite runs shared fixtures against both SDKs and fails CI on
divergence; any unavoidable difference is an explicit, reviewed exception
(§2). Implemented by.

Host decision (October 20, 2026): Python loads the identical Node guest
WASM in Wasmtime, without a second detector/policy implementation. Python names
use snake_case; ParamSpec/metadata preservation replaces JS callable descriptors,
and bound-method receivers replace `this`. A ContextVar-backed context manager
scopes already guarded calls, including async tasks. Pydantic models retain
identity and are inspected only through detached data. TS-specific framework and
HTTP adapters are outside the Python primitive slice. Shared fixture comparison
normalizes these language representations while checking actual call counts,
identity, exceptions, mode snapshots, halts and bypass events.

Python delivery detail: the default NullEngine alone may use the guest's fused
`null_call` ABI. Rust executes both phases per invocation, and refuses selected
engines on this path; no policy decision is cached or implemented in Python.
Selected engines use the same suspended request/response session as Node.
Python's ready value is a boolean after synchronous local loading. A sole
positional payload is inspected directly; multiple arguments become an array;
keyword calls use an args/kwargs envelope. Normal descriptor-bound receivers
are excluded from inspection but forwarded unchanged. Python currently mirrors
the callable `wrap` surface; framework/client/HTTP adapters, Node OTel, the CLI
and sandbox codegen remain explicit limits of the Python primitive slice.
The shared corpus and one-sided Python mutations execute in the relay test task.

### ✅ 12.5 Adapter install failure — DECIDED

**Decision: degrade per entry; the app always starts. Nothing to fall back to → loud setup error.**

A tool entry that can't be wrapped, or an adapter that throws while attaching,
unwraps and warns; the app always starts. When the failure leaves nothing to
fall back to — an ambiguous target, invalid config — it throws at setup instead
(§12.1). Both paths are loud; neither is silent (§6).

### 12.6 Frontend and hosted control plane — PLANNED

Decision (October 23, 2026): the hosted agent surface is a separate
read-only Worker at the configured production HTTPS origin, with expiring
operator-issued tenant credential digests, secret-backed Ed25519 signing and an
independently distributed SPKI fingerprint. D1 evidence is published off-path;
every SELECT binds the authenticated tenant, and simulation dispatches nothing.
The Worker uses ahead-of-time frozen validators and a Worker-compatible MCP
schema validator. Public machine mirrors activate only through a reviewed,
revision-bound publication input after live checks, using path-only redirects.
Missing configuration refuses hosted requests; this control-plane refusal never
affects guard availability. Deployment, live qualification, OAuth-dependent
directory admission and public listings require separate retained evidence.

Decision (October 23, 2026): public, dashboard and private identity
responses share a restrictive browser policy: HSTS, no framing, same-origin
opener/resource isolation, bounded permissions, no-referrer and CSP with
same-origin connections. Private responses are `private, no-store` and vary on
cookies. This deployment profile exposes no credentialed CORS allowlist; foreign
origins and preflights are refused, while mutations retain exact Origin, JSON,
CSRF-header and Fetch Metadata checks. Host-only Secure HttpOnly SameSite cookies,
server-validated redirects, escaped Solid text/inert bootstrap JSON, tenant-bound
cursors/caches/exports, HMAC-only API-key persistence, secret-reference delivery
and pinned-address SSRF controls are exercised by the retained adversarial suite.
Missing host owners and hosted evidence keep affected routes unavailable. This is
a local qualification, not penetration-test or hosted TLS certification. See
.

Decision (October 23, 2026): the organization-wide data page consumes
's owner/session privacy API directly through the dashboard identity
binding. Desired consent and effective acknowledgement remain separate. Approved
purpose, limits and inventory dispositions are authoritative; absent host policy
never becomes a default retention period. Export/deletion job IDs are retained
only for the current page because exposes no job-list route, and no job,
capability or payload is persisted in browser storage. Deletion requires typed
organization confirmation plus the existing recent-authentication/MFA gate.
Legal holds, partial receipts, retained evidence, failure and expiry remain
visible without fabricating erasure or completion. Download grants are requested
only for completed unexpired exports and used immediately in a same-origin
navigation. Hosted policy, owner adapters, artifact storage and deletion
qualification remain explicit.

Decision (October 23, 2026): private session-only review summary/detail
reads project 's durable ledger. Connection-bound detail URLs disambiguate
case IDs across connections. Original admission SLA, bounded complete evidence,
actual audit patch state and immutable four-action resolution receipts remain
separate from runtime authorization. Actor/time are server-owned; notes and
expected revisions use the existing idempotent resolution command. Uncertain
sends retry the identical feedback command, while reviewer conflicts require
refresh. Observe-mode shadow ingestion, bulk approval and promotion remain
unavailable without safe durable owner commands. No customer write or terminal
key is replayed or released by feedback. See.

Decision (October 23, 2026): notification pages use a validated local
presentation seam. Now owns the private
`GET|POST /api/orgs/{org}/environments/{test|live}/notifications` boundary:
reads require `evidence.read`, commands require `notifications.manage`, and both
authorize before owner-store access. Reads are cursor/limit bounded and expose
redacted destination/key metadata plus ledger acceptance state. Commands bind
actor, expected revision and idempotency key. Registration reuses /
SSRF, challenge, verification and suppression policy. Test/redrive is rate-limited;
only dead-letter/expired deliveries may redrive, through a notification-only port.
Signing material is AES-GCM envelope encrypted under an explicit versioned
server-held wrapping-key port; reads/logs never expose ciphertext or plaintext,
and new plaintext is returned only by successful issue/rotation. The default host
injects neither service nor wrapping key, so production administration remains
unavailable and is still required.

Decision (October 23, 2026): the private
`GET /api/orgs/{org}/environments/{test|live}/analytics` route authorizes
`evidence.read` before querying one hourly UTC-aligned, at-most-31-day range.
Admission deduplicates by organization/environment/call ID and conflicts on
different evidence under that identity. Call totals never sum findings. Each
measure declares unit, source/sample coverage, qualification, watermark and,
for rates, numerator/denominator/cohort. Percentiles require admitted raw latency
samples. A sampled source returns `total_traffic_source_unavailable` for total
traffic measures rather than zero. Billing stays a separate source. The
default host supplies no source and the browser source remains unavailable.

/ decision (October 23, 2026): performance targets, a pinned mobile
profile, per-route JS/query/render budgets and deploy-scoped synthetic ownership
are versioned local evidence. Analytics is a separate client chunk. Telemetry is
an optional off-path OTel-shaped sink with only request ID, route template,
release, bounded status/HTTP status and duration; faults are contained and no
query, tenant, payload, form text or evidence enters it. With no compatible trace
tool, lab/field values remain unmeasured rather than invented. The release matrix
enables a route only with contract/component/E2E evidence, same-environment
backend and all feature signoffs; current hosted blockers keep dashboard and
advanced rows unavailable, and dashboard failure cannot affect guarded calls.

Decision (October 23, 2026): usage/billing consume commercial
summaries, with authorized command availability and approved plan names. Session
returns may use the exact originating organization/environment billing path;
return parameters never confirm payment. Up to 20 incident references
include one eligible audit locator per incident, with explicit unavailable and
truncated states; private receipt hashes and logical write keys stay private.
Audit retention may expire independently of accounting. Commercial confirmation
and usage read time do not assert ingestion completeness: the meter has no
watermark. UI labels possible delay explicitly. Billing never disables universal
Tier 1 detection or grants runtime repair authority.

Decision (October 23, 2026): `audit_customer/v1` is the concrete server
allowlist for customer audit reads and exports. It retains approved frozen v1
facts and typed summaries, removes arbitrary metadata and payload-bearing fields,
and preserves retained scalar lexemes without numeric rounding. The shared route
enforces it for all sources. Configuring `AuditReadSource` automatically supplies
this policy and the SQL export receipt writer; `createAuditAccess` pairs them
as options. Owner callbacks may restrict further but cannot bypass the
customer allowlist. Deployment still needs authoritative archive partitions,
coverage/retention facts, migration 020 and approved receipt governance inventory.

Decision (October 23, 2026): connection pages consume reads and
Revisioned settings commands. Confirmation binds mode/protection/reason
to the inspected desired revision; acceptance stays pending until the runtime
acknowledges it. Every false protection request, including strict and retries,
requires recent authentication. Desired/effective opt-out epochs and
cumulative counts remain separate through restoration and refresh failures.
Missing runtime metadata remains unavailable. Settings retries reuse their
command identity; no dashboard control retries a customer write. Response-halt
guidance states the upstream write may already have completed. Host delivery,
readiness sources and hosted qualification remain explicit.

Decision (October 23, 2026): contract/invariant pages use a local typed
presentation seam for scoped immutable evidence. Production supplies no source
until freezes its missing read/command transport. Generic string
projections cannot supply signed-pin verification, local-window readiness or
activation authority. Tests explicitly inject synthetic evidence; none is wired
into application builds. All administrative controls remain disabled pending
Server authority, expected revisions, confirmation, durable receipts and
runtime acknowledgement. Evidence remains distinct from repair authorization;
qualification, authority and active protection are independently presented.

/ decision (October 23, 2026): scoped v2 audit artifact GET and export
POST subroutes return exact retained frozen v1 text with digest, owner provenance,
redaction and expiry headers. Export uses existing session `evidence.read`
authority, same-origin CSRF and a durable attributed preparation receipt before
bytes are released. The POST uses `Content-Type: application/json` and an empty
object body (`{}`), matching the identity gateway's mutation contract; additional
fields are refused before archive access. Schema and patch/recurrence honesty are validated without
rewriting numeric lexemes. Original producer bytes/bodies and upstream outcomes
are never reconstructed. Full artifacts require explicit owner approval/redaction;
SQL receipts, source configuration, governance inventory and hosted qualification
remain explicit host gates. No frozen schema is extended for these routes.

/ decision (October 23, 2026): shared `dashboard_query` and
`dashboard_read` advance to v2 for richer drift filters and typed independent
findings. `/api/dashboard/v2/orgs/{org}/environments/{env}/drift` uses the existing
session authorization, indexed owner sources and bounded HMAC cursor snapshots.
V1 endpoints retain the closed string projection and status-only filter.
V2 drift adds side, wire class, detector family, connection, mode, outcome,
severity and identifier-only incident/audit search. Findings retain independent
owner IDs and explicit evidence availability; invariant is a detector family,
never a fourth wire class. No raw payload or repair proof is inferred. Sources
without authoritative typed data remain unconfigured; hosted qualification is
still explicit.

October 20, 2026: and the
Record the public/dashboard boundary.
Public pages use `agindow.com`; dashboard/auth routes use `dash.agindow.comguard` signature or frozen message changes here.

October 23, 2026: adds a read-only console investigation host port.
Every queue/detail request checks evidence-read authority in the exact
organization/environment before source I/O. Bounded scoped projections retain
owner state/reasons, evidence references, UTC timestamps, integer micro-USD cost
receipts and validated completion hypotheses. Missing orchestration remains
unavailable; no command authority, active repair rule or invariant is implied.
No default owner adapter or hosted/provider qualification is supplied.

October 22, 2026: freezes editorial `agindow.docs/1`: a source-owned
`contracts/docs` tree keyed by `(family, version, slug)` feeds both public docs
and surface Markdown. A reviewed digest binds tree, generated text and shared
HTTP description; both builds execute published examples against the existing
local MCP interface. This matrix is local and unpublished, not a released SDK
claim. New SDK snippets require executable released-interface validation. Public
copy excludes tickets, cells and internal architecture. Source-reviewed availability
inputs gate machine links, support and keys; request flags do not enable them.
The public keys landing requires both management and dashboard releases and must
select an authenticated organization/environment before the scoped key route.
Status reads an optional bounded host port per request, expires observations in
five minutes, and aborts a stalled source after one second; unavailable never
asserts health. Changelog routes come only from reviewed published dated entries.
No hosted endpoint, support channel, monitor or key issuance is configured.

October 23, 2026: –82 publish the Get started, Guides, API and SDK
inventories from the same editorial tree. Registry publication is independent of
local usability: validate Node snippets against built declarations and execute
Node/Python/CLI local distributions, with labelled synthetic traffic and audit
sinks. Console/surface builds explicitly depend on the relay build solely for
these checks. Canonical readable docs URLs preserve versioned aliases. Technical
SDK documentation may name NullEngine and local build paths; marketing maturity
copy still excludes implementation inventories. API operation bodies project the
exact signing source, including the existing OpenAPI and Markdown GET routes.
The separate execution host is linked explicitly rather than claiming the
read-only surface serves POST /v1/guard. Signature verification requires an
independently trusted key and removes x-agindow-signature before canonicalization.

October 22, 2026: adds public contact, demo and launch-interest forms and
discoverability metadata. Intake definitions are source-owned and rendered per
request so availability reflects the actual host binding. POSTs require exact
`Origin`, form encoding, bounded fields/body, a honeypot and process-local,
hashed-client rate/duplicate limits; validation errors never echo submitted text.
A success receipt exists only after an optional `INTAKE` service binding accepts
a versioned frame and returns a valid receipt within the source-bounded 180-day
retention deadline. No local fallback store is shipped; an absent binding or
downstream fault produces an explicit failure, never fabricated success. Optional
conversion analytics is an unchecked per-submission choice and can receive only
an allowlisted event name; URLs, emails, messages, field values and secrets are
excluded, and no analytics provider is configured. Metadata uses the configured
public origin (not the request Host), canonical/OG fields only on indexable
production documents, and JSON-LD without ratings, offers or unsupported claims.
Trust and dashboard documents are always noindex; preview/staging robots disallow
all crawling; production serves a finite sitemap. Hosted intake durability,
provider wiring, abuse-state distribution and production qualification remain
release prerequisites.

October 22, 2026: freezes the v1 identity/membership matrix in its ticket.
The private registry identity Worker mounts Better Auth 1.7.12 on the dashboard
origin via a service binding; AGINDOW organization IDs remain provider-independent.
Tenant mutations lock the organization and recheck current membership in the same
Postgres transaction. Invitations require verified matching email, retain only
HMAC digests, and are accepted once. Ownership transfer requires authentication
within five minutes plus the configured host MFA policy. Dashboard mutations
require exact Origin, JSON and `X-Agindow-CSRF: 1`, with no CORS allowlist.
Email/password is the configured sign-in method; public signup is disabled and
host provisioning triggers verification. Local tests do not qualify hosted email,
provider delivery, or production deployment. No relay/SDK login is introduced.

October 22, 2026: adds local authenticated dashboard list/detail reads
under `/api/dashboard/v1/orgs/{orgId}/environments/{test|live}`. The frozen
`dashboard_query`, `dashboard_read` and `dashboard_error` v1 schemas preserve
owner status/string lexemes and explicit capability/freshness/retention states.

freeze 31-day ranges, five-minute freshness/snapshots, indexed candidate budgets,
HMAC scope-bound cursors and 1 MiB responses. Snapshots prevent concurrent inserts
from shifting pages; expired/restarted cursors refuse instead of restarting.
Authorization precedes source access. Owner adapters must supply environment
mapping, coverage and retention facts; no hosted source is enabled by default.
This adds no in-path dependency or runtime guard change.

October 22, 2026: / freeze scoped fallback key management under
`/api/orgs/{orgId}/environments/{test|live}/api-keys` on the private identity
Worker. Keys use an environment prefix, 32-hex public ID and 64-hex random secret,
with versioned tenant/environment-derived HMAC verification material. Creation
and rotation reveal the secret once; 24-hour command replays contain metadata
only. Sensitive commands reuse 's fresh-session/MFA policy. Revocation is
irreversible, rotation overlap is 60 seconds–24 hours, and cached verification or
signed local snapshots expire within 60 seconds without sliding refresh.
The v1 scope registry is code-owned and initially empty. Keys remain explicit
fallback credentials; no existing producer/reader, SDK or guard boundary opts in.
Off-path verifier faults return 503; stale local state bypasses once per call.
Local snapshot hosts receive only scoped non-extractable CryptoKeys through an
explicit background provisioning port; master material is never distributed.
Future boundary tickets own concrete scope registration and host qualification.

October 22, 2026: freezes the private connection-control command routes,
single-runtime authority, durable receipt/outbox protocol and bounds in its
.
Trusted background enrollment/count/admission ports remain separate from session
commands. Accepted desired revisions, acknowledged effective revisions and original
opt-out epochs are distinct. The background adapter contract requires idempotent
application and exact receipt acknowledgement; no default SDK wiring is implied.
The connection owner projection uses explicit private cursor material and a bounded
read cache across request-local identity handlers. It carries desired/effective
opt-out epochs separately so a pending restoration never hides applied opt-out
counts. Native mode/protection reloads are independent; a runtime host must supply
atomic publication of the complete snapshot before acknowledging the pair.

October 22, 2026: freezes organization-scoped owner governance commands,
approved inventory/policy inputs, durable jobs and scoped expiring download grants
in its.
No statutory retention defaults are inferred; missing policy or enforcement ports
leave accepted jobs pending. Organization deletion suspends only that tenant's
access; shared user accounts and other tenants' sessions remain separate.

October 22, 2026: selects Stripe hosted Checkout and Billing Portal and
freezes the commercial v1 routes, approved catalog, durable session intents and
verified background reconciliation in its
.
A redirect never confirms a purchase. Billing reads combine confirmed durable
commercial state with the incident ledger. Billing cannot authorize repair
or disable detection; no provider call enters a guard. Host provisioning and
Stripe sandbox/hosted qualification remain explicit.

Administrative mode/protection/review/contract commands are attributed,
revisioned and delivered in the background. Durable acceptance and effective
runtime acknowledgement are distinct. Every call keeps its existing immutable
snapshot, and no guard synchronously depends on this control plane. Human
review never executes a write or releases terminal idempotency keys.

October 22, 2026: supersedes
the initial brand-color frontend palette with black and white only, including
buttons, labels, statuses, charts and generated mono logos. System sans-serif UI
text and monospace technical text remain; forced colors honor OS choices. Public copy, pricing, charts and evidence pages
must follow the actual backend capability and qualification state; missing
measurements are unavailable. The linked plan and tickets specify release gates.

October 22, 2026: adds five local static draft trust pages and permanent
privacy choices.
has no legal entity/contact, effective policy or deployed inventory/provider/transfer
approvals. Public production Worker initialization refuses independently of feature
flags; preview/staging drafts remain noindex. Public operation is essential-only:
no optional scripts, cookies, storage or consent receipt. New optional processing
requires a qualified consent mechanism; no legal approval or hosted qualification
is inferred from these local disclosures.

October 23, 2026: supplies local dashboard document authorization against
fresh private session context and an equivalent `/api/session/shell` read. Root
selects the first authorized organization in test; each API still checks its own
current permissions. Switchers clear visible context, abort reads and replace the
document; generation checks ignore late responses. A validated, escaped current-path
bootstrap preserves accepted warnings during client startup. The explicit off-path
protection host port requires all accepted scoped receipts, authoritative counts,
stale-warning retention and effective restoration; it is unconfigured in the
default Worker. No date-filtered audit feed may substitute for standing protection
state. Page bodies, durable host wiring and hosted qualification remain separate.

October 23, 2026: adds `/onboarding` with anonymous key-free source guides
and session-gated organization creation/resume. Test/live remain namespaces; setup
always selects test. Scoped resume URLs choose a deployment; a source-owned catalog
gates released commands and test-key scopes (none enabled by default). The optional
off-path readiness adapter must authenticate and scope runtime observations before
projecting connectivity, authenticated first evidence, baseline, detector and
effective mode independently. Stale observations remain labelled; connection/list
presence and desired state are not readiness. Conditional key POSTs retain
server authority and one-time/idempotent semantics. No hosted collector, released
package, production promotion or in-path dependency is added.

October 23, 2026: adds dashboard test/live key administration over the
existing private session routes. A successful keys.manage list enables UI
controls; every write still checks current backend authority/recent-auth/MFA.
Masked metadata, explicit UTC expiry/rotation overlap and reviewed live commands
remain separate from one-time secret reveal/copy. Uncertain retries retain one
idempotency intent, conflicts refresh without replay, and metadata-only responses
cannot retrieve secrets. Close, history, scope change and disposal clear the panel;
late reads cannot restore it. Scope options start empty like the registry; no
producer credential, dashboard key login or in-path dependency is added.

October 23, 2026: preserves these retry and verification decisions across
refreshes and blocked background work. Governance blocks and owner-pending steps
defer retries by 30 seconds without consuming failure attempts; due jobs sort by
retry eligibility before creation order. Same-scope onboarding refreshes erase
displayed secrets while retaining the issuance command. Mixed-mode key boundaries
check each requested scope's declared mode in both verifier implementations;
local-only scopes never trigger database lookup. Default host availability stays
unchanged.

### ✅ 12.7 embedded distribution boundary — DECIDED

Ships `cells/cell-relay/node` as an independently packable `agindow`
package (Node ≥22), with ESM/CJS, declarations, licenses and a release Rust guest.
It provides `createAgindow`, the default `agindow`, `wrap`, `guard`, `configure`
and `ready`. The SDK wraps plain functions and maps of functions or plain tool objects
with `func`/`execute`; platform discovery and dedicated client/HTTP adapters remain
. Fetch/undici/axios-based tool implementations are ordinary callables; raw
Response/stream objects pass through untouched and are not consumed for inspection.

Loading is asynchronous and local. `ready` always settles; failed loading leaves
calls bypassed. Strict does not wait inside synchronous calls: await `ready`
explicitly before starting protected work. There is no automatic retry or egress.
Default detection stays NullEngine. Explicit caller-supplied statistical windows
select the path; JSON inspection copies are bounded and never replace values.
No automatic baseline collection is enabled. Unsupported objects bypass inspection.

Mode resolution for this distribution is explicit > `AGINDOW_MODE` > observe.
`configure` atomically updates subsequent calls; each call freezes its options,
engine and local selection for both phases. File discovery belongs to.
A caller may deliver config changes from its own runtime control channel. No
background polling or filesystem reads occur on the call path.

The production guest uses wasm-bindgen's generated memory boundary and retains
call-owned request/response state across asynchronous host execution. A host
monotonic timestamp feeds WASM readiness; no guest clock/network/thread is used.
Any guest trap permanently retires that instance; later calls bypass until an
explicit replacement. JS outcomes remain in the host; only bounded inspection
copies enter WASM. A response halt carries the completed outcome and a warning
against blind replay. The SDK loop benchmark qualifies only its declared fixture;
Owns the full statistical/bypass matrix and owns artifact draining.

---

## 12.8 offline adapter and sandbox boundaries

October 20, 2026: config files are read at SDK construction, never on a call.
The Node default remains observe/NullEngine. `buffer` defaults to 100 (maximum
100,000), spans use the optional application OTel API, and scoped-out tools
emit a bypass without engine evaluation. Adapter failures retain the original
entry and warn. Class tools with a named schema and `invoke` use that executor;
unsupported branded classes remain unchanged. LangChain ToolCall arguments are
projected before inspection; its output formatting remains the framework's.
Class client helpers bind their original receiver to retain private state,
while plain-object helpers retain exact function identity. Mixed HTTP/tool
function maps require an explicit adapter.

HTTP adapters retain original Response/end bytes. Fetch inspection requires a
complete JSON body with a bounded content length; Express inspects complete
JSON `end` chunks only. Chunked, unknown-length and already-streamed bodies skip
response inspection. No incoming Node stream is consumed to inspect it; parsed
request bodies require host middleware before this boundary. Verified raw
response replacements may change bytes and invalidate content length/encoding
and entity checksums. SDK repair output is accepted only from the trusted engine
seam, with verification and confidence gates; the guest does not supply it.

`simulate` runs the same local request/response phases against a recording.
Partial or unavailable captures return bypass, and no upstream callable runs.
`report` parses JS/TS without evaluating it, skips unsupported syntax/symlinks,
and reads only an explicit local snapshot. Historical drift-event envelopes
are validated with the frozen contract. They can describe repair candidates,
but cannot authorize a new repair or establish that a current payload is clean.
The real-Nango-project waiver was revoked on October 22; the pinned upstream
report run is recorded in. Imported
Nango action/sync factories use `nango/<relative source path without extension>`
identities. Conventional `integrations/<integration>/actions|syncs/` paths supply
integration labels, not remote-provider proof. No registry HTTP service is assumed.

Sandbox codegen is a separate, explicitly selected limited gate, required by
Nango's no-npm boundary. Its local generation input is versioned and file/tool
scoped. Only caller-confirmed same-type sibling renames may repair requests;
semantic type/enum changes never become automatic conversions. No learned
invariants, statistics, write-integrity proof or hosted audit is implied.
The generated TS runtime is one source for execution and sandbox simulation.
Request checks run before the provider call; response checks run afterward.
No environment-variable fetch or escalation network call is added on the
guard path. `AGINDOW_DISABLED=true` is a generation-time opt-out; a live
sandbox environment transport remains a separate integration requirement.
Local snapshots are supplied explicitly until registry delivery exists.

---

October 23, 2026: adds organization-wide bounded settings reads and
owner-only naming, self-only Better Auth display-name updates, and invitation
resend with recipient-digest matching and atomic token rotation. Existing
membership locks, ceilings and immutable events remain authoritative. Password
and session revocation commands share ownership transfer's recent-auth/MFA gate;
sign-out remains available. Browser focus/history restoration revalidates session
and membership instead of restoring cached tenant data. No SSO, domain or MFA
enrollment capability is enabled. See the.

### 12.9 — explicit remote callable

`guardRemote(original, envelope, options)` returns `Promise<Response | T>`.
`original(envelope)` owns credentials and receives the unchanged idempotency key.
Options require `endpoint`; optional `timeoutMs`, `apiKey`, and `onEvent` control
transport. This API is explicit because a synchronous tool must not silently
become asynchronous when an endpoint config is present. Local `guard` retains
its synchronous contract. A remote connection failure is not proof a write
wasn't sent; upstream idempotency is the caller binding's responsibility.

### 12.10 — opt-in guest audit publication

`createAgindow({ audit: { tenant, capacity?, maxBytes? } })` enables the guest
outbox. `await sdk.drainAudit(async artifact => sink(artifact), limit?)` returns
`{ delivered, pending, dropped, available }`. A failing sink stops the drain and
retains the head ID; concurrent drains return immediately without duplicate
sink entry. No sink runs on the guarded call path. Default configuration neither
constructs audit artifacts nor performs egress. Restart loses in-memory pending
artifacts; persisted sink IDs survive only if the host persists them.

### 12.11 — internal guest payload encoding

The `agindow` host boundary passes captured JSON to the release guest as
UTF-8 bytes. The generated esm/cjs typings change from `string` to `Uint8Array`
for the internal `Guest.begin`/`GuestCall.finish` parameters; this is not part
of the public wrapper surface and does not change `agindow.guard`, adapters or
the HTTP envelope. Python already supplies UTF-8 bytes through the same
pointer/length ABI. Host capture keeps `capture` returning a JSON string for
existing consumers and adds a byte projection for the guest; escaping, byte
caps and truncation semantics are identical. Recorded because measured a
nonzero host-capture cost at the previous string crossing and requires the
boundary decision to precede the optimization.

### 12.12 — explicit native credential observation and operator cutover

The native host opts into `agw_guard::credential::session::Session` after an
upstream response. It supplies non-secret identity tokens, credential version,
availability/completeness and a bounded capture; original streaming bytes and
status pass through, and the host drains at most two additive events separately.
This does not activate a new Node/Python SDK or default guest detector.

Operator code creates `Cutover` with opaque active/candidate `Arc` handles and
an explicit bounded probe plan. `Dispatcher` captures one immutable binding per
call. The host owns network timeouts and probe retries; inconclusive non-credential
HTTP outcomes consume the configured probe budget, cannot promote, and do not
report a dead credential. Exhaustion, timeout, execution faults or credential
failure abort verification. Unsupported receipts carry `unsupported_reason`
(`atomic_invalidation` or `unknown_capability`). The host feeds candidate outcomes,
drives overlap ticks and persists snapshots before releasing old bindings.

See [the credential guide](/cells/cell-relay/CREDENTIALS.md) for the
local API, offline readiness input and verification boundaries.

### 12.13 — machine identity (2026-10-22)

The per-surface authority, TTL, skew, proof, fault and revocation decisions are
[contract §10.56](/agindow-runtime-contract.md). `RemoteOptions.auth`
accepts a prepared in-memory session; setup/background refresh owns identity
acquisition and exchange. Guard calls read the cached credential and create a
local proof only. Initial acquisition failure is `AgindowSetupError`; runtime
unavailability follows, with one bypass and the original binding.
An explicit legacy key remains a compatibility input, labelled legacy; there is
no implicit fallback from a failed keyless session. Embedded calls are unchanged.
Registration adapters are authenticated control-plane host responsibilities;
local exchange/verifier APIs have no network or database dependency. Python
uses an injected signer so private-key custody remains with the host; Node can
create an ephemeral Ed25519 signer in process. Algorithm/provider limitations
and incomplete adoption must remain visible in the identity guide.

Open follow-up: must settle the
distributed replay authority, ownership fencing and restart/re-authentication
protocol before implementation. Per-call shared-store lookups are not authorized
by the current local-validation contract; no global replay guarantee is implied.

## 13. Tickets

| # | Deliverable | Notes |
|---|---|---|
| | `agindow.guard` primitive + config types | Foundation for everything |
| | Event system, ring buffer, OTel spans | |
| | `toolAdapter` + property tests for metadata preservation | LangChain + Mastra |
| | `clientAdapter` + explicit skip-list | Nango, Pipedream |
| | `httpAdapter` + streaming passthrough | Only true byte-identity path |
| | `agindow.simulate` | Sales + CI + eval |
| | `agindow.report` CLI | **Highest leverage.** Ship early. |
| | `agindow.wrap` auto-detect | After (c)+(d) land |
| | Python parity + cross-language contract tests | |
| | Nango zero-dep codegen | See |

**Order matters: (report) before (Nango).** The CLI creates demand while the integrations catch up. Nango-only sub-tickets (skill, template PRs, docs, launch, OEM) continue the letter sequence from `` in §9.

---

*Part of· Contract: [`../../../the runtime contract](/agindow-runtime-contract.md)*
