# AGINDOW SDKs and in-path runtime

**API drift inspection for AI agent calls.** Configure evidence windows to
detect changes, then inspect findings without changing calls in observe mode.

This repository is the home of the installable SDKs **and** of the complete
Apache-2.0 runtime that inspects, blocks or modifies your API calls: the eleven
Rust crates, the WASM guest, the native/sidecar and Worker hosts, the Node and
Python SDK sources, the public wire contracts and the synthetic evaluation
fixtures.

It is a **source mirror** of that runtime (see [CONTRIBUTING.md](./CONTRIBUTING.md)).
Hosted service backends and dashboard administration are not included in this
source mirror.

| Package | Registry | Install |
|---|---|---|
| [`agindow`](https://www.npmjs.com/package/agindow) | npm | `npm install agindow@0.1.0-alpha.2` |
| [`agindow`](https://pypi.org/project/agindow/) | PyPI | `pip install agindow==0.1.0a1` |

Both are **pre-alpha** (`0.1.0-alpha.2` / `0.1.0a1`). Apache-2.0. The default
statistical guest needs explicit scoped baseline/live windows. It does not
supply selective repair. Follow the [Node or Python detection quickstart](https://agindow.com/docs/quickstart)
for a verified local finding and controls; the setup examples below only prove
wrapping. Neither a fixture nor installation proves production protection.

## Quick start (Node)

```bash
npm install agindow@0.1.0-alpha.2
```

```js
import { createAgindow } from 'agindow';

const sdk = createAgindow({ mode: 'observe', buffer: 100 });
await sdk.ready;
const run = sdk.guard(() => ({ ok: true }), { tool: 'vendor.read' });
console.log(run());
```

CLI (after install):

```bash
npx agindow report ./src
```

## Quick start (Python)

```bash
pip install agindow==0.1.0a1
```

```python
from agindow import Agindow

sdk = Agindow(mode="observe")

@sdk.guard(mode="observe", tool="vendor.read", operation="read")
def read_vendor():
    return {"ok": True}

print(sdk.ready, read_vendor())
sdk.close()
```

## Examples

```bash
# Node (Node ≥22)
cd examples/node && npm install && node observe.mjs
# labelled local detection (Unit finding): node detect.mjs

# Python (3.11+)
cd examples/python && pip install -r requirements.txt && python observe.py
```

Examples depend only on the published packages.

## Runtime source

| Path | Contents |
|---|---|
| `cells/cell-relay/crates/` | eleven Rust crates: guard, inspector, diff, repair, stats, invariants, policy, audit, notify, node-guest, http-host |
| `cells/cell-relay/wasm/` | WASM-facing TypeScript seams |
| `cells/cell-relay/hosted/`, `sidecar/` | Worker build and Unix/loopback sidecar hosts |
| `cells/cell-relay/node/`, `python/` | SDK sources, tests, parity fixtures and examples |
| `cells/cell-relay/public-benchmark/`, `perf/` | synthetic evaluation and latency tooling |
| `contracts/` | frozen wire schemas, validators, examples and baselines |
| `agindow-runtime-contract.md` | normative runtime behavior and availability rules |
| `wrapper-api.md` | shared Node/Python integration surface and adapter contract |

Build, lint and test from a clean clone: [BUILD.md](./BUILD.md). The gates run
the same Rust workspace, WASM guest and SDK test suites used before each alpha
release.

## Contributing and security

Issues and pull requests for the published packages and runtime live here — see
[CONTRIBUTING.md](./CONTRIBUTING.md). Report vulnerabilities privately per
[SECURITY.md](./SECURITY.md), never in a public issue.

## Docs and product

- Website / docs: [https://agindow.com](https://agindow.com)
- Package READMEs ship inside each install (also mirrored in releases)

## What this repo is not

- Does not include hosted service backends or dashboard administration
- Not a claim of production detection SLA or calibrated defaults
- Not tested against live vendor traffic

## License

Apache-2.0 — see [LICENSE](./LICENSE) and [NOTICE](./NOTICE).

## Public documentation and coding-agent resources

Start with [Quickstart](https://agindow.com/docs/quickstart) and the
[capability matrix](https://agindow.com/docs/capabilities). For Markdown, use
[/docs.md](https://agindow.com/docs.md), the public developer manual.
[/llms.txt](https://agindow.com/llms.txt) is the compact page index;
[/llms-full.txt](https://agindow.com/llms-full.txt) adds full documentation and
coding-agent guidance. These mirrors may return 503 on the public origin until
an evidence service is enabled; the browser docs remain the starting point.

The public [SKILL.md](https://agindow.com/SKILL.md) and
[spore.md](https://agindow.com/spore.md) describe integration without enabling it
automatically. [MCP tools](https://agindow.com/docs/api/tools) read evidence and
simulate proposals; they never execute vendor writes. Signed
[/openapi.json](https://agindow.com/openapi.json) belongs to a configured MCP
origin, not an embedded SDK requirement or a managed execution endpoint.
