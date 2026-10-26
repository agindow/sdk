# AGINDOW SDKs and in-path runtime

**API drift firewall for AI agents** — repair drift in-flight, prove the write
was correct.

This repository is the home of the installable SDKs **and** of the complete
Apache-2.0 runtime that inspects, blocks or modifies your API calls: the eleven
Rust crates, the WASM guest, the native/sidecar and Worker hosts, the Node and
Python SDK sources, the public wire contracts and the synthetic evaluation
fixtures.

It is a **source mirror** of that runtime (see [CONTRIBUTING.md](./CONTRIBUTING.md)).
The shared corpus, profile generation, hosted registry/control plane, Cortex,
dashboard backends and operations are not part of this tree.

| Package | Registry | Install |
|---|---|---|
| [`agindow`](https://www.npmjs.com/package/agindow) | npm | `npm install agindow` |
| [`agindow`](https://pypi.org/project/agindow/) | PyPI | `pip install agindow` |

Both are **pre-alpha** (`0.1.0-alpha.1` / `0.1.0a0`). Apache-2.0. The default
engine detects nothing until you configure protection.

## Quick start (Node)

```bash
npm install agindow
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
pip install agindow
```

```python
from agindow import Agindow, guard

@guard(mode="observe", tool="vendor.read", operation="read")
def read_vendor():
    return {"ok": True}

sdk = Agindow(mode="observe")
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

- Not the registry, shared corpus, Cortex, identity, dashboard backend or
  hosted operations
- Not a claim of production detection SLA or calibrated defaults
- Not qualified against live vendor traffic

## License

Apache-2.0 — see [LICENSE](./LICENSE) and [NOTICE](./NOTICE).
