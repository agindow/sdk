# Changelog

## 0.1.0-alpha.1 / 0.1.0a0

- This repository now ships the complete Apache-2.0 in-path runtime source
  (eleven relay crates, WASM guest, native/sidecar/Worker hosts, Node and Python
  SDK sources, public contracts, synthetic fixtures) with a clean-clone
  `pnpm check` gate; see [BUILD.md](./BUILD.md)
- First public alpha: unscoped **`agindow`** on npm and PyPI
- Keywords added for registry discovery
- Prebuilt WASM guest; observe-mode NullEngine by default (no detection until configured)
- Node: ESM/CJS, CLI (`agindow`), tool/client/HTTP adapters, offline report/codegen
- Python: decorators, contexts, remote fail-open API; identical WASM bytes to Node
