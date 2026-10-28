# Changelog

## 0.1.0-alpha.2 / 0.1.0a1 — Published October 28, 2026

[GitHub release notes](https://github.com/agindow/sdk/releases/tag/v0.1.0-alpha.2).

Published on npm and PyPI. Both packages include opt-in reviewed selective repair
and the same shared WASM guest.

- Statistical guest by default, with an empty plan until windows are selected.
- Opt-in selective repair in Node/Python: reviewed tool/side-scoped top-level
  field renames, shared Rust span patches and independent synchronous verification.
  Repair remains off by default; enable rules and repair/strict mode explicitly.
- Independently runnable Node/Python configured detection examples and clearer
  readiness, bypass, configuration, audit and write-safety documentation.
- Updated SDK READMEs, public machine-document formats and coding-agent guidance.
- Node 22+ and Python 3.11+; identical shared WASM guest across both packages.

See the [SDK changelog](https://agindow.com/docs/changelog) for upgrade guidance.

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
