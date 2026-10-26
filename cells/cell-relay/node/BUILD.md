# Building `agindow` (maintainers)

This file is **not** published to npm. App developers use
`npm install agindow` — see [README.md](./README.md).

Rebuild the WASM guest and package artifacts from the AGINDOW monorepo root:
```bash
pnpm install
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.129 --locked

node cells/cell-relay/node/scripts/build.mjs
node --test cells/cell-relay/node/tests/*.test.mjs
pnpm exec vitest run --config cells/cell-relay/node/vitest.config.ts

node cells/cell-relay/node/examples/pipedream.mjs
node cells/cell-relay/node/examples/langchain.mjs
node cells/cell-relay/node/examples/offline.mjs

cd cells/cell-relay/node && npm pack
# npm publish --access public
```
`AGINDOW_WASM_BINDGEN` selects an alternate generator binary. Pack size must
stay under 5,000,000 bytes unpacked (including WASM). Publish checklist:
.
