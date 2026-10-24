# AGINDOW SDKs

**API drift firewall for AI agents** — repair drift in-flight, prove the write
was correct.

This repository is the **public** home for the installable SDKs: issues,
changelog, and examples. The packages ship **prebuilt** Rust/WASM guests; you
do not need Rust or the private AGINDOW monorepo to use them.

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

# Python (3.11+)
cd examples/python && pip install -r requirements.txt && python observe.py
```

Examples depend only on the published packages.

## Docs and product

- Website / docs: [https://agindow.com](https://agindow.com)
- Package READMEs ship inside each install (also mirrored in releases)

## What this repo is not

- Not the Rust engine source, Workers, identity, or dashboard
- Not a claim of production detection SLA or calibrated defaults
- Issues here are for the **published SDKs**; security reports: follow the
  process on [agindow.com](https://agindow.com) / security contact when listed

## License

Apache-2.0 — see [LICENSE](./LICENSE) and [NOTICE](./NOTICE).
