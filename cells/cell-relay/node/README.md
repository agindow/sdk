# agindow

API drift firewall for AI agents on Node — repair drift in-flight, prove the
write was correct. **Pre-alpha** on npm as `agindow` (Node ≥22,
Apache-2.0, ESM/CJS and TypeScript declarations). Runs a Rust/WASM engine in
process. OpenTelemetry is an optional peer dependency.

```ts
import { createAgindow } from 'agindow';

const sdk = createAgindow({ mode: 'observe', buffer: 100 });
await sdk.ready;
const tools = sdk.toolAdapter(existingTools);
sdk.configure({ mode: 'repair' });
const unsubscribe = sdk.on('bypass', event => console.warn(event));
```

`guard(fn, { tool, operation })` preserves sync/async behavior, argument arrays,
receiver, result identity and original exceptions. `toolName` resolves a dynamic
identity. `inputShape: 'raw'` admits a sole string/byte argument without JSON
reserialization. Original errors survive; admitted policy feedback enriches them
as `AgindowDriftError.cause`. Response halts retain the completed `outcome` and
warn against blind replay. Unavailable drift/audit IDs remain empty, never made
up. Bypass status is delivered through events, never injected into returned data.

The default remains **observe / NullEngine: no drift detection or repair**.
Loading is asynchronous and local. `ready` always settles. Calls while loading,
after failed loading, on unsupported captures or after a guest trap bypass once
and invoke upstream at most once. Strict also bypasses while loading; await
`ready` and monitor bypasses before relying on protection. Trapped instances
remain retired. Create a new SDK instance to replace one.

## Configuration and events

Precedence is explicit > environment > nearest JSON `.agindowrc` > defaults.
Files are read once at construction, never on the call path. Invalid/unreadable
files warn and are ignored; invalid explicit/environment values are named setup
errors. Supported environment keys: `AGINDOW_MODE`, `AGINDOW_KEY`,
`AGINDOW_ENDPOINT`, `AGINDOW_SIDECAR`, `AGINDOW_BUFFER`, `AGINDOW_SPANS`,
`AGINDOW_FAIL_CLOSED` and `AGINDOW_DEBUG`. Boolean values are `true`/`false`.

Mode updates affect later calls; a call freezes its configuration and engine
through promise settlement. Operations on generic callables default to read;
set write/unknown explicitly for writes. HTTP adapters infer GET/HEAD/OPTIONS as
read and other methods conservatively as unknown. Client methods use GET as
read and the other allow-listed methods as unknown unless explicitly configured.
`failClosed: false` warns; semantic writes and strict retain precedence.
`allowedTools` defaults to `'*'`; deny wins over allow. Scoped-out calls bypass.

`on('drift' | 'repair' | 'blocked' | 'bypass' | 'error', callback)` returns an
unsubscribe function. Events are immutable bounded metadata and include tool,
mode, side/phase, latency, available class/confidence/audit ID and computed repair
byte identity. Listener faults cannot release policy or replace upstream results.
`recent` returns the retained events, oldest first. The default ring holds
100 events, `buffer: 0` disables retention, and the maximum is 100,000.
Internal errors emit an error event and at most one bypass per call.

Spans open by default through the application's optional `@opentelemetry/api`;
its provider controls sampling/export. `spans: false` disables them. No provider
means no-op spans. API/provider faults never fail a guarded call.

The typed `apiKey`, `endpoint`, `sidecar`, `sync`, `headers`, `transformers` and
`debug` configuration fields do not activate a hosted service, polling,
transport, header injection or a transformer pipeline.

## Adapters

- **`toolAdapter`** — maps and arrays of plain `func`/`execute`/`invoke`
Definitions and maps of functions. Keys, symbols, opaque schema identity,
Descriptions and flags survive. Stateful executors retain their original tool
Receiver. Named class tools with a schema use `invoke` (including LangChain
  ToolCall formatting); unrecognized branded classes remain untouched with a
Warning. Batch/stream framework paths are not qualified by the invoke adapter.
- **`clientAdapter(client, { tool })`** — wraps only `get`, `post`, `put`,
  `patch`, `delete`, `request`, `fetch`, `proxy`. Storage/auth helpers never
Enter guard. Use Pipedream's `client.proxy` surface; wrapping its whole
Container does not discover nested clients.
- **`httpAdapter(fetch)`** — retains the original Response on the no-repair path.
  Inspects bounded JSON with known content length (at most 1 MiB).
  `httpAdapter(expressApp)` inspects complete JSON `end` chunks; streaming
  `write` paths skip response inspection.
- **`wrap`** — selects supported tool/client/HTTP shapes. Mixed HTTP/tool maps
Throw `AgindowSetupError` with explicit adapter alternatives. Ordinary
Functions reduce to `guard`.

Verified replacements require the trusted engine seam, explicit verification
and confidence gates. Observation never applies a replacement. The statistical
WASM guest does not supply selective repair by default.

Nango proxy/client calls use `clientAdapter` / `httpAdapter`; action/sync
scanning uses the `agindow scan` CLI. Composio/Merge/Paragon closed catalogs
cannot host this SDK.

## Offline simulation, report and CLI

`simulate(tool, { args, recorded, mode, operation, partial })` runs only local
engine phases. Await `ready` first. It never invokes an upstream callable or
fabricates repair confidence.

After `npm install agindow`, the CLI binary is **`agindow`**:

```bash
npx agindow report./src
npx agindow report./src --snapshot./local-drift.json
npx agindow codegen./local-contracts.json
npx agindow scan./nango-project --snapshot./local-contracts.json
```

Report never executes scanned source or writes a project file. Missing evidence
is unavailable, not a clean result. `AGINDOW_DISABLED=true` is a generation-time
opt-out. `AGINDOW_BRANDING=off` omits the CLI banner at build time.

The sibling Python package `agindow` uses the identical WASM artifact.

## Explicit statistical windows

`createAgindow({ selection })` binds tenant, connection, vendor, operation and
segment to at most seven field windows. Each specifies side, an object-only JSON
Pointer, complete raw JSON baseline/live arrays (1–4096 samples each), and an
explicit enumeration boolean. A tool ID must exactly match `selection.operation`;
other tools bypass. Callers own sampling independence and scope correctness.
There is no automatic collection or remote refresh.

Ordinary copies are bounded to 1 MiB, 4096 nodes and 64 levels. `work` defaults
to 200 million cooperative units (not a wall-clock deadline). Durable replay and
hosted audit delivery remain separate work.

## Install

```bash
npm install agindow
```

The package includes the prebuilt WASM guest. You do **not** need Rust, a
monorepo checkout, or a local engine build to use the SDK.

## Remote execution and audit drain

`guardRemote(original, envelope, { endpoint, apiKey?, timeoutMs?, onEvent? })`
returns an untouched successful HTTP Response. It falls back exactly once on
5xx/transport failures and emits one bypass event. Write/unknown envelopes require
an upstream-enforced `idempotency_key`. Local synchronous guards never acquire a
remote dependency.

```js
const sdk = createAgindow({ audit: { tenant: 'example', capacity: 64 } });
await sdk.ready;
const result = sdk.guard( => ({ value: 1 }), { tool: 'vendor.read' });
const receipt = await sdk.drainAudit(async artifact => {
  // Persist artifact.id durably before acknowledging.
Console.log(JSON.stringify(artifact));
});
```

Audit publication is disabled without explicit options. Default bounds are 64
records / 512 KiB serialized (maximum 256 / 1 MiB, 128 KiB per artifact). Hosts
own persistence and retry; no durable SDK audit guarantee is claimed. Opt-in
machine-identity helpers stay credential-free in embedded mode; they do not
enable a hosted identity service by themselves.
