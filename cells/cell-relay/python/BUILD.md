# Building `agindow` (maintainers)

This file is **not** published to PyPI. App developers use `pip install agindow`
— see [README.md](./README.md).

The Python wheel embeds the **same** WASM bytes as `agindow`. Build the
Node guest first, then the wheel, from the AGINDOW monorepo root:

```bash
pnpm install --frozen-lockfile
node cells/cell-relay/node/scripts/build.mjs
python3 cells/cell-relay/python/scripts/build.py --stage-only
uv sync --directory cells/cell-relay/python --frozen
uv run --directory cells/cell-relay/python --frozen mypy
uv run --directory cells/cell-relay/python --frozen python -m unittest discover -s tests
node cells/cell-relay/node/parity/run.mjs
uv run --directory cells/cell-relay/python --frozen python scripts/build.py
# twine upload dist/*
```

Requires Rust `wasm32-unknown-unknown` and wasm-bindgen CLI **0.2.129**.
Publish checklist: [SDK.md](../../../SDK.md).
