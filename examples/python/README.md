# Python examples

These examples install the published package from PyPI — no local Rust build is
needed.

```bash
python -m venv .venv && source .venv/bin/activate   # optional
pip install -r requirements.txt
python observe.py
```

`pyrightconfig.json` points the type checker at that virtual environment. This is a wrapper setup fixture: it uses observe mode without a detector
selection and preserves the original object. It does not demonstrate drift
detection. Run the Python tab of the [configured quickstart](https://agindow.com/docs/quickstart)
for a real local finding and unchanged/unconfigured/insufficient-window controls.
Python readiness is a boolean; it is not awaitable.

Public Markdown guidance is available through the [document index](https://agindow.com/llms.txt)
and [developer manual](https://agindow.com/docs.md) when those mirrors are enabled;
use the browser quickstart if they return 503. The [integration skill](https://agindow.com/SKILL.md)
is coding-agent guidance, not automatic detector setup.
