# agindow (Python)

Inspect API calls made by AI agents for changes in data shape and meaning on
Python 3.11+. **Pre-alpha** package for PyPI as `agindow`. Loads the **identical
WASM bytes** built for `agindow` through Wasmtime. Python provides
callable/configuration/availability glue — not a second drift engine.

The statistical guest is default. Configure explicit scoped baseline/live windows
to detect drift. Selective repair is off by default. Enable reviewed rename rules and an independent verifier explicitly.

Start with the Python tab of the [configured detection quickstart](https://agindow.com/docs/quickstart):
save it as `detect.py` and run `python detect.py`. It asserts one local finding
and unchanged/unconfigured/insufficient-window controls. The Python `ready`
property is boolean, not awaitable.

The embedded SDK needs no separate AGINDOW credential. Keep vendor credentials
in your existing client. These fixtures do not establish production accuracy or
persisted write correctness. See [capabilities](https://agindow.com/docs/capabilities).

## Install
```bash
pip install agindow==0.1.0a1
# or: uv add agindow==0.1.0a1
```
The wheel includes the prebuilt WASM guest. You do **not** need Rust, Node, or a
monorepo checkout to use the SDK.

## Decorators, async and contexts

This is a setup fixture, not detection. Configure selection as in Quickstart
to produce a finding; selecting repair or strict does not configure a detector.
```python
from agindow import Agindow, guard

@guard(mode="repair", tool="customers.create", operation="write")
def create_customer(email: str) -> dict[str, str]:
    return {"email": email}

sdk = Agindow(mode="observe")

@sdk.guard(tool="customers.read", operation="read")
async def read_customer() -> dict[str, str]:
    return {"id": "fixture"}

with sdk.context(mode="strict"):
    # Only calls through this SDK's guards use the scoped configuration.
    pass
# `async with sdk.context(...)` is supported as well.
sdk.close()
```
Context managers scope guarded calls; they cannot intercept arbitrary Python
statements. Overrides use ContextVar, nest and remain isolated across async
tasks. Each call freezes configuration until completion. `configure(mode="repair")`
changes future calls. `sdk.ready` is a synchronous boolean after local loading
(unlike Node’s startup Promise); `sdk.load_error` retains a loading failure.
Loading failures and retired guests preserve upstream results and emit one
bypass per call. `close` is safe for shutdown; subsequent calls bypass.

The wrapper preserves `__name__`, documentation, annotations, signatures,
bound receivers and arguments. ParamSpec retains Pydantic model types without
a mandatory Pydantic dependency. Model results retain their original identity;
only a detached `model_dump(mode="json")` projection enters inspection.

## Configuration and feedback

Supported fields (snake_case): `mode`, `fail_closed`, `api_key`, `endpoint`,
`allowed_tools` (`"*"` or a tuple), `deny_tools`, `buffer` and `on_event`.
Resolution: explicit keywords > environment > nearest JSON `.agindowrc` >
defaults. Shared env vars match Node (`AGINDOW_MODE`, `AGINDOW_KEY`,
`AGINDOW_ENDPOINT`, `AGINDOW_BUFFER`, `AGINDOW_FAIL_CLOSED`). Invalid explicit
or environment values raise `AgindowSetupError` before a call.
```python
from agindow import AgindowDriftError

try:
    create_customer("fixture@example.test")
except AgindowDriftError as error:
    print(error.code, error.tool, error.drift_class, error.drift_id)
    print(error.hint, error.audit_url)
```
Request halts dispatch zero times. Response halts retain `error.outcome` and
warn against blind replay. Upstream exceptions survive as `error.__cause__`.
Missing audit IDs/URLs remain empty rather than fabricated.

`on(type, handler)` returns an unsubscribe callback; `recent` returns an
immutable tuple of retained events. `buffer=0` disables retention. This is local
telemetry, not hosted audit delivery.

## Shared-core validation and limits

Parity fixtures run both public wrappers against the same WASM guest. The
paired local agent-loop benchmark gates added p50 &lt;100 µs on the default
Unselected statistical plan only (excludes model/network/import/selected-detector work).

Python `wrap` currently accepts callables. Node-specific tool/client/HTTP
adapters, Nango codegen, OTel spans, report CLI and remote configuration/audit
transports are outside this package. No Python framework adapter or streaming
qualification is claimed. Inspection is bounded to detached JSON projections.

## Remote fail-open API

Local decorators stay offline. Remote execution is a separate HTTP API using
stdlib HTTP transport; it never redirects or retries through AGINDOW:
```python
from agindow import RemoteEnvelope, RemoteOptions, guard_remote

request = RemoteEnvelope("vendor.read", "example", "read", {}, "observe")
result = guard_remote(
    lambda envelope: {"fallback": True},
    request,
    RemoteOptions("http://127.0.0.1:8789/v1/guard"),
)
```
`RemoteResponse` retains the original status/headers and streaming `body`.
Any 5xx/refusal/timeout/reset invokes the original callable once and emits one
bypass. Writes/unknown operations require the same upstream idempotency key in
the envelope and fallback. `guard_remote_async` runs the blocking API in a
worker thread for a synchronous fallback callable.

Opt-in machine-identity helpers stay credential-free in embedded mode; they do
not enable a hosted identity service by themselves. Sibling Node package:
`agindow`.

## Maintained detection example

The complete [`detect.py`](./examples/detect.py) matches the published quickstart byte for byte.
Run it after installing the pinned package in your environment. The documentation
publication gate executes this same source against matching built SDK artifacts.

## Public documentation and coding-agent resources

Start with [Quickstart](https://agindow.com/docs/quickstart) and the
[capability matrix](https://agindow.com/docs/capabilities). For Markdown, use
[/docs.md](https://agindow.com/docs.md), the public developer manual.
[/llms.txt](https://agindow.com/llms.txt) is the compact page index;
[/llms-full.txt](https://agindow.com/llms-full.txt) adds full documentation and
coding-agent guidance. These mirrors may return 503 on the public origin until
an evidence service is enabled; the browser docs remain the starting point.

The public [SKILL.md](https://agindow.com/SKILL.md) and
[spore.md](https://agindow.com/spore.md) describe integration without enabling it
automatically. [MCP tools](https://agindow.com/docs/api/tools) read evidence and
simulate proposals; they never execute vendor writes. Signed
[/openapi.json](https://agindow.com/openapi.json) belongs to a configured MCP
origin, not an embedded SDK requirement or a managed execution endpoint.

## Enable selective repair

Repair is off by default. Construct `Agindow` with
`selective_repair=SelectiveRepair(rules=(RenameRule(...),), verify=verify)`.
`RenameRule` identifies `tool`, request/response `side`, top-level `source` and
`target` keys, reviewed `migration` ID and `confidence`. The local synchronous
verifier receives immutable `RepairCheck.before`, `.after` JSON strings and
`.rule`; it must check intended meaning using independent authoritative data.
Load that data outside calls. Start in `Config(mode="observe")`, then use
`sdk.configure(mode="repair")` after evaluation; observe disables application.

The shared Rust engine prepares selective key edits. Existing statistical
findings/blocks remain in force. Numeric, nested and array migrations are outside
this option. See the [runnable example and limits](https://agindow.com/docs/sdk/configuration#enable-selective-repair).
