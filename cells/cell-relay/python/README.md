# agindow (Python)

> **Published release:** Node `0.1.0-alpha.3` on npm and Python `0.1.0a2`
> on PyPI. Both are pre-alpha; the pinned install commands below are available.

**Dependabot for agents. At runtime.**

Detect API drift as agents run, apply reviewed and verified runtime repairs, and turn drift evidence into validated source fixes and draft pull requests.

Pre-alpha. Detection requires configured windows. Runtime repair is opt-in for supported changes. Source-fix tooling is available locally; hosted PR automation is not yet qualified.

Use Python 3.11+ to observe existing API calls and explicitly configure supported
detection or verified repair. The embedded SDK needs no separate AGINDOW key;
vendor authentication stays in your application. The statistical guest needs
explicit scoped baseline/current windows, and selective repair is off by default.

## Install
```bash
pip install agindow==0.1.0a2
# or: uv add agindow==0.1.0a2
```
The wheel includes the prebuilt WASM guest. You do **not** need Rust, Node, or a
monorepo checkout to use the SDK.

## First API call

Save the complete program below as `orders.py`. Set `API_URL` to your existing
read-only HTTPS JSON endpoint and run `python orders.py`. Keep authentication,
headers and vendor credentials in your current client when adapting the function.
```python
import json
import os
from urllib.request import urlopen
from agindow import Agindow

sdk = Agindow(mode="observe")
sdk.on("drift", lambda event: print("Drift:", event.verdict))

@sdk.guard(tool="orders.get", operation="read")
def get_order() -> object:
    with urlopen(os.environ["API_URL"], timeout=10) as response:
        order: object = json.load(response)
        return order

try:
    print("API result:", get_order())
    print("AGINDOW events:", [event.type for event in sdk.recent()])
finally:
    sdk.close()
```
Expect the API response and `AGINDOW events: []` with no detector selected. An
empty list does not prove drift-free traffic. `sdk.ready` is a boolean, not
awaitable; `sdk.load_error` retains a load failure. A failed engine load preserves
the original call through bypass. Upstream HTTP/network/JSON errors propagate.
The finally block closes this same SDK instance.

## Async and scoped configuration

Decorate an `async def` with `@sdk.guard(...)` and await it normally. The complete
[async Python example](https://agindow.com/docs/sdk/python#guard-an-async-operation)
uses an application-owned HTTP call. Sync/async scopes use
`with sdk.context(mode="observe"):` / `async with sdk.context(mode="observe"):`.
See the [Python API reference](https://agindow.com/docs/sdk/python-reference) for
complete examples, arguments and failure contracts.

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

Catch `AgindowDriftError` only for policy handling; inspect `code`, `tool`,
`drift_class`, `drift_id`, `hint`, `audit_url` and the original cause as applicable.
See [errors and retained outcomes](https://agindow.com/docs/guides/errors).

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
adapters, Nango codegen, OTel spans, report CLI and guest audit
draining are outside this package. No Python framework adapter or streaming
qualification is claimed. Inspection is bounded to detached JSON projections.

## Remote fail-open API

Local decorators stay offline. Remote execution is a separate HTTP API using
stdlib HTTP transport; it never redirects or retries through AGINDOW:

Use `guard_remote` / `guard_remote_async` only with a configured execution host
and your real original-call fallback. Follow the complete
[remote guide](https://agindow.com/docs/sdk/remote) for a labelled local protocol
exercise, options, authentication, response cleanup and fallback semantics.
A fabricated fallback value is not a successful vendor result.

`RemoteResponse` retains the original status/headers and streaming `body`.
Any 5xx/refusal/timeout/reset invokes the original callable once and emits one
bypass. Writes/unknown operations require the same upstream idempotency key in
the envelope and fallback. `guard_remote_async` runs the blocking API in a
worker thread for a synchronous fallback callable.

Opt-in machine-identity helpers stay credential-free in embedded mode; they do
not enable a hosted identity service by themselves. Sibling Node package:
`agindow`.

## Maintained detection example

The complete [`detect.py`](./examples/detect.py) matches the published detection example byte for byte.
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
Load that data outside calls. Start with `Agindow(mode="observe",...)`, then use
`sdk.configure(mode="repair")` after evaluation; observe disables application.

The shared Rust engine prepares selective key edits. Existing statistical
findings/blocks remain in force. Numeric, nested and array migrations are outside
this option. See the [runnable example and limits](https://agindow.com/docs/sdk/configuration#enable-selective-repair).
