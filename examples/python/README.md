# Python examples

These examples install the published package from PyPI — no local Rust build is
needed.

```bash
python -m venv .venv && source .venv/bin/activate   # optional
pip install -r requirements.txt
python observe.py
```

`pyrightconfig.json` points the type checker at that virtual environment. The
default engine observes only; `observe.py` prints the guard outcome so you can
see that the original object is returned unchanged.
