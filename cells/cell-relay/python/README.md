# agindow (Python)

API drift firewall for AI agents on Python 3.11+ — repair drift in-flight, prove
the write was correct. **Pre-alpha** on PyPI as `agindow`. Loads the **identical
WASM bytes** built for `agindow` through Wasmtime. Python provides
callable/configuration/availability glue — not a second drift engine.

The default engine detects nothing until you configure protection. Statistical
windows require explicit local baseline/live selections.

## Install

```bash
pip install agindow
# or: uv add agindow
```

The wheel includes the prebuilt WASM guest. You do **not** need Rust, Node, or a
monorepo checkout to use the SDK.

## Decorators, async and contexts

```python
from agindow import Agindow, guard

@guard(mode="repair", tool="customers.create", operation="write")
def create_customer(email: str) -> dict[str, str]:
    return {"email": email}

sdk = Agindow(mode="observe")

@sdk.guard(tool="customers.read", operation="read")
async def read_customer -> dict[str, str]:
    return {"id": "fixture"}

with sdk.context(mode="strict"):
    # Only calls through this SDK's guards use the scoped configuration.
    pass
# `async with sdk.context(...)` is supported as well.
sdk.close
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
NullEngine only (excludes model/network/import/statistical work).

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
