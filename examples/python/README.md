# Python examples

These examples install the pinned package from PyPI — no local Rust build is
needed.

```bash
python -m venv .venv && source .venv/bin/activate   # optional
pip install -r requirements.txt
python observe.py
```

Set `API_URL` to an existing read-only HTTPS JSON endpoint before running
`observe.py`. The program uses the same SDK instance for the decorator, events
and cleanup. It prints the API response and recent event kinds. Keep vendor
authentication in your own client when adapting the function. An empty event
list is expected without a selection; it does not establish drift-free traffic.
Python readiness is a boolean, not awaitable; failed loading retains load_error
and guarded calls bypass. Original HTTP/network/JSON errors propagate.

`pyrightconfig.json` points the type checker at the virtual environment. For a
labelled local finding and stable/unconfigured/insufficient-window controls, use
the [detection guide](https://agindow.com/docs/detection). It makes no live vendor
call. The pinned release is available from PyPI.

Public Markdown guidance is available through the [document index](https://agindow.com/llms.txt)
and [developer manual](https://agindow.com/docs.md) when those mirrors are enabled;
use the browser quickstart if they return 503. The [integration skill](https://agindow.com/SKILL.md)
is coding-agent guidance, not automatic detector setup.
