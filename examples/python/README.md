# Python examples

```bash
pip install -r requirements.txt   # after PyPI publish
python observe.py
```

**In this monorepo (before PyPI):** create a local env so the IDE resolves `agindow`:

```bash
uv venv .venv
uv pip install --python .venv/bin/python -e ../../../cells/cell-relay/python
source .venv/bin/activate   # or select .venv in the editor
python observe.py
```

`pyrightconfig.json` points at `.venv` and the monorepo `src` path.
