# TI-31i shared wrapper contract tests

`cases.json` is the single fixture definition. `typescript.mjs` and the Python
SDK's `scripts/parity.py` execute their actual public wrappers and emit one
normalized row per fixture. Cases include scripted engine-seam behavior and
actual shared-WASM statistical decisions, reads, observe mode and exhausted
budgets. `compare.mjs` rejects missing, duplicate, extra and divergent rows.

After building/staging both packages and syncing the locked Python environment,
run this from the checkout:

```bash
node cells/cell-relay/node/parity/run.mjs
```

The relay test task runs the comparison in CI. Node tests mutation-check the
comparison itself. Python tests copy its actual wrapper, disable a halt or make
bypass throw, and require the cross-language command to fail. No external
service or fake Python engine implementation replaces the production binding.

Normalization checks call counts, result/error identity, retained outcome,
mode/classification, sync/async behavior, receiver/argument preservation and
bypass counts. Python uses snake_case, bound-method receivers and a synchronous
ready boolean after local loading. These language differences and the Python
primitive slice's limits are documented in wrapper API §12.4 and the Python
SDK guide. Framework/HTTP adapters remain Node surfaces.

This runner requires the repository's sibling Python project; the Node tarball
alone does not contain that distribution. The Python wheel independently
packages the same WASM bytes and does not require Node at runtime.

`api-keys.json` contains public, non-production TII-38 key-format fixtures. Registry,
Node and Python tests consume the same forms; TI-70 fallback parsers and keyless
precedence are unchanged. This does not register a hosted scope or enable keys.
