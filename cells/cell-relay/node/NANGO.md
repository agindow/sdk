# Nango integration — local pre-alpha

The package is buildable from this repository and is not published to npm.
The Node SDK requires Node 22 or later. The generated action/sync code imports
no npm package and does not load WASM. No hosted AGINDOW route is required.

| Surface | Setup | Current coverage | Limits |
|---|---|---|---|
| A: agent layer | `sdk.clientAdapter(client, config)` | Explicit HTTP methods on a Node client | Proxy calls only. `triggerAction` is not an HTTP allow-list method; guard it explicitly to inspect action output. Provider calls inside actions remain outside this wrapper. |
| B: proxy script in your Node runtime | `sdk.httpAdapter(fetch, config)` | Completed bounded JSON response bytes, or explicit client methods | No inspection of chunked/non-JSON/unknown-length streams; no incoming Node stream repair. This is the customer's Node runtime, not a sandboxed action. |
| C: action/sync | `agindow scan project --snapshot contracts.json` | Scalar shape checks, string enums, explicitly confirmed request sibling renames | No statistics, learned invariants, automatic conversions, hosted audit, global consistency or write-integrity proof. |

A and B use the Rust/WASM distribution. Its default remains NullEngine and
detects nothing. Statistical protection requires explicit scoped baseline/live
windows. The default guest has no selective-repair binding; the wrapper's
verified-repair seam is available to trusted engine integrations. Installing a
wrapper alone does not establish protection or qualify a latency budget.

## A: guard proxy calls
```ts
import { Nango } from '@nangohq/node';
import { createAgindow } from 'agindow';
const sdk = createAgindow({ mode: 'observe' });
await sdk.ready;
const client = new Nango({ secretKey: process.env.NANGO_SECRET_KEY ?? '' });
const proxy = sdk.clientAdapter(client, {
  tool: (request: { endpoint: string }) => `nango/${request.endpoint}`,
});
```
`get`, `post`, `put`, `patch`, `delete`, `request`, `fetch` and `proxy` are the
complete method allow-list. `auth`, `connect`, `disconnect`, `batchSave`,
`getCheckpoint` and `saveCheckpoint` never enter drift evaluation. Class helpers
bind their original receiver; plain-object helpers keep function identity.
Opaque or cyclic SDK response envelopes remain unavailable for inspection.
For those clients, guard a callable returning the provider JSON value, or the
supported Fetch transport boundary.
An action-output callable can be guarded explicitly with `sdk.guard` and an
explicit tool/operation. It cannot inspect a provider call hidden inside Nango.

## B: guard a Node proxy script
```ts
import { createAgindow } from 'agindow';
const sdk = createAgindow({ mode: 'observe' });
await sdk.ready;
export const guardedFetch = sdk.httpAdapter(fetch, {
  tool: 'vendor.operation', operation: 'read',
});
```
No-repair results retain the original Response and stream. Complete JSON
inspection uses a bounded copy, not a replacement body. A verified repair can
produce a new body and reports `byte_identical: false`. Body bytes exposed by
Fetch may already have been decoded by the runtime; this is not a claim about
compressed bytes on the upstream wire. Passthrough notifications are events;
they do not change the upstream body/status. A response halt retains completed
outcome evidence and warns against blind replay.

## C: generate the local action/sync gate

Build the checkout, then inspect the generator's demonstration input:
```bash
node cells/cell-relay/node/scripts/build.mjs
node cells/cell-relay/node/dist/cli.cjs report ./my-nango-project
node cells/cell-relay/node/dist/cli.cjs codegen cells/cell-relay/node/examples/nango-contracts.json
node cells/cell-relay/node/dist/cli.cjs scan ./my-nango-project --snapshot ./my-local-contracts.json
```
After packing/installing the local tarball, `agindow` is the same CLI. The
example contract is fictional test evidence, not a claim about Stripe's API.
Supply your own authoritative identity/type evidence before adding a rename.
Generation inputs are local version-1 objects with a `contracts` array, one
file/tool/method/literal-endpoint contract per file. Each snapshot is at most
2 KiB. Actions and syncs must use imported `createAction`/`createSync`, an `exec`
context receiver and a literal endpoint. Unsupported shapes are counted and
left unchanged. Re-run `scan`: unchanged contracts produce zero changed files.
Refresh from a new local snapshot; no live registry-fetch command is supplied.

Request checks run before the matching provider call; response checks run
afterward. A semantic response halt cannot undo an already completed write.
Response errors use the context's `ActionError`, with structured feedback and
an explicit replay warning. The generated gate adds no network, credential
lookup, environment-variable fetch or escalation request. Internal unavailable
input passes through and warns. Root objects and scalar fields are supported;
arrays, deep paths, opaque values and incomplete captures are unavailable.

`AGINDOW_DISABLED=true agindow scan...` is an explicit generation-time
opt-out. Re-generate without it to restore the snapshot's configured setting.
This does not poll a live sandbox environment variable. Runtime opt-out delivery
and hosted escalation remain unimplemented.

The gate repairs parsed values. A rename changes what Nango serializes, so the
repair path does not claim raw byte identity and never asserts write integrity.
Historical drift events alone do not authorize a rename. The generated runtime
is substantially larger than the early eight-line sketch; no template adoption
or deployed sandbox qualification is claimed. Hosted audit artifacts require a
separate implemented delivery path; this gate has no such path.

## Validation and distribution

Offline fixtures exercise actions/syncs, idempotence, no imports/network, the
request-before-dispatch boundary and response semantic blocking. The user waived
the real-Nango-project report run on October 20, 2026. A deployed Nango dryrun
has not been performed. The builder skill ships under
[skills/guarding-nango-functions/SKILL.md](./skills/guarding-nango-functions/SKILL.md).
Partner publication, template review, a joint launch and OEM agreement remain
pending; see the.

The sandbox restrictions and error convention were checked against Nango's
[function-building skill](https://github.com/NangoHQ/skills/blob/main/skills/building-nango-functions/SKILL.md).
Template redistribution must retain the upstream
[Elastic License 2.0 and modification notices](https://github.com/NangoHQ/integration-templates/blob/main/LICENSE).
The Apache-2.0 shim does not change the templates' license.
