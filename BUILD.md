# Building and testing from source

This repository is self-contained. The published packages ship prebuilt WASM
guests, so you only need this guide if you are auditing or changing the runtime.

## Prerequisites

| Tool | Version | Why |
|---|---|---|
| Node | ≥ 22 | SDK builds and tests |
| pnpm | `packageManager` in `package.json` | workspace install |
| Rust | `rust-toolchain.toml` (stable, edition 2024) | crates and guest |
| `wasm32-unknown-unknown` | via `rustup target add` | WASM guest |
| `wasm-bindgen-cli` | `0.2.129` | guest bindings |
| Python | ≥ 3.11 | Python SDK tests |
| `uv` | current | locked Python environment |

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.129 --locked
```

## Gate

```bash
pnpm install --frozen-lockfile
pnpm build   # cargo build --workspace, WASM guest, Node dist, Python guest staging
uv sync --directory cells/cell-relay/python --frozen
pnpm lint    # cargo fmt/clippy/doc, tsc, mypy
pnpm test    # cargo tests, WASM/Node/perf suites, Python unittest, cross-language parity
```

Build before `uv sync`: the Python package force-includes `_guest.wasm`, which
only exists after the Node guest build stages identical bytes into
`cells/cell-relay/python/src/agindow/`. `pnpm check` runs build → lint → test;
run `uv sync` once after the first build on a clean clone. A release candidate
must pass this sequence before npm/PyPI publication.

## What each step builds

| Step | Output |
|---|---|
| `cargo build --workspace` | native crates, including `agw-http-host` |
| `node cells/cell-relay/node/scripts/build.mjs` | `node/dist/` (ESM + CJS + declarations + CLI) and the release guest |
| `python3 cells/cell-relay/python/scripts/build.py --stage-only` | copies the identical guest bytes into the Python package |
| `pnpm test` | Rust workspace, guest, Node, Python and parity suites |

The Node and Python SDKs must ship the **same** WASM bytes. The build fails if
the two bindgen targets drift; rebuild both together.

## Focused runs

```bash
cargo test -p agw-guard                       # one crate
pnpm exec vitest run --config cells/cell-relay/node/vitest.config.ts -t "guard"
uv run --directory cells/cell-relay/python --frozen python -m unittest discover -s tests
node cells/cell-relay/node/parity/run.mjs
node cells/cell-relay/perf/run-availability.mjs
```

## Packaging notes

- `cells/cell-relay/node`: `npm pack --dry-run` must stay under 5 MB unpacked.
- `cells/cell-relay/python`: `uv run --frozen python scripts/build.py` writes the
  wheel and sdist under `python/dist/`.
- The statistical guest is default; configure windows to detect. Installing or building does not enable
  detection. Explicit selections are required, and no build here qualifies
  detection against production traffic.

## Repository layout

Path layout is load-bearing: Rust crates compile `contracts/examples/*.json`
with `include_str!` at a fixed relative depth. Moving `contracts/` or
`cells/cell-relay/crates/` breaks the build.

- `contracts/` — frozen wire schemas, validators, examples and baselines
- `cells/cell-relay/` — the runtime cell (crates, hosts, SDKs, fixtures)
- `agindow-runtime-contract.md`, `wrapper-api.md` — public behavior specifications
- `examples/` — install-from-registry samples
- `skills/` — agent integration guidance

The standalone export omits admission tests that require private dashboard or
registry cells, and the combined admission mutation runner. Those cross-cell
checks remain in the monorepo. The public admission runtime, policy and SDK tests
remain available through `pnpm --filter cell-relay test:admission`.
