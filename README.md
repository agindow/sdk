# AGINDOW SDKs and in-path runtime

> **Published release:** Node `0.1.0-alpha.3` on npm and Python `0.1.0a2`
> on PyPI. Both are pre-alpha; the pinned install commands below are available.

**Dependabot for agents. At runtime.**

Detect API drift as agents run, apply reviewed and verified runtime repairs, and turn drift evidence into validated source fixes and draft pull requests.

Pre-alpha. Detection requires configured windows. Runtime repair is opt-in for supported changes. Source-fix tooling is available locally; hosted PR automation is not yet qualified.

Configure evidence windows to detect changes, then inspect findings without
changing calls in observe mode.

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
| [`agindow`](https://www.npmjs.com/package/agindow) | npm | `npm install agindow@0.1.0-alpha.3` |
| [`agindow`](https://pypi.org/project/agindow/) | PyPI | `pip install agindow==0.1.0a2` |

Both are **pre-alpha** (`0.1.0-alpha.3` / `0.1.0a2`). Apache-2.0. The default
statistical guest needs explicit scoped baseline/live windows. Selective repair
is off by default; enable reviewed top-level rename rules and an independent
verifier explicitly. Follow the [Node or Python detection guide](https://agindow.com/docs/detection)
for a verified local finding and controls; the setup examples below only prove
wrapping. Neither a fixture nor installation proves production protection.

## Quick start (Node)

```bash
npm install agindow@0.1.0-alpha.3
```

Use Node 22.6+ for the native TypeScript command. Save as `orders.mts`, set `API_URL` to your existing read-only HTTPS JSON
endpoint, then run `node --experimental-strip-types orders.mts`. Embedded use
needs no AGINDOW key; retain vendor credentials in your own client.

```ts
import { createAgindow } from 'agindow';

const sdk = createAgindow({ mode: 'observe' });
await sdk.ready;
sdk.on('drift', event => console.log('Drift:', event.verdict));

const getOrder = sdk.guard(async () => {
  const url = process.env.API_URL;
  if (!url) throw new Error('Set API_URL to your order JSON endpoint.');
  const response = await fetch(url, { signal: AbortSignal.timeout(10_000) });
  if (!response.ok) throw new Error(`API returned ${response.status}`);
  const order: unknown = await response.json();
  return order;
}, { tool: 'orders.get', operation: 'read' });

const order = await getOrder();
console.log('API result:', order);
console.log('AGINDOW events:', sdk.recent().map(event => event.type));
```

From your installed application project with an existing `./src` directory:

```bash
npx --no-install agindow report ./src
```

## Quick start (Python)

```bash
pip install agindow==0.1.0a2
```

Save as `orders.py` with the same API_URL and run `python orders.py`.

```python
import json
import os
from urllib.request import urlopen
from agindow import Agindow

sdk = Agindow(mode="observe")
sdk.on("drift", lambda event: print("Drift:", event.verdict))

@sdk.guard(tool="orders.get", operation="read")
def get_order() -> object:
    with urlopen(os.environ["API_URL"], timeout=10) as response:
        order: object = json.load(response)
        return order

try:
    print("API result:", get_order())
    print("AGINDOW events:", [event.type for event in sdk.recent()])
finally:
    sdk.close()
```

Both programs print the original API response and recent event kinds. With no
selection or repair rules, `AGINDOW events: []` is expected; it is not a clean
bill of health. Original API/JSON errors propagate. Node ready settles after a
loading attempt; Python ready is a boolean and load_error retains a failure.
Bypass preserves the call when inspection is unavailable.

For configured findings, see [Detection](https://agindow.com/docs/detection).
For opt-in reviewed repair and full APIs, use the
[Node](https://agindow.com/docs/sdk/node) or
[Python](https://agindow.com/docs/sdk/python) guide.

## Examples

```bash
# Node (Node ≥22)
cd examples/node && npm install && node observe.mjs
# labelled local detection (Unit finding): node detect.mjs
```

In a separate terminal from the repository root:

```bash
# Python (3.11+)
cd examples/python && pip install -r requirements.txt && python observe.py
```

Run each language in a separate terminal from this repository root; replace
API_URL with your own endpoint before observe.mjs/observe.py. The pinned packages
are published for these registry installs. Detection fixtures remain
separately labelled and make no vendor calls.

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
