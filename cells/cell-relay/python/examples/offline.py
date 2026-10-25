"""Fictional local evidence: no credentials, vendor calls or production claims."""
from __future__ import annotations

import json
from agindow import Agindow, AgindowDriftError

selection: dict[str, object] = {
    "tenant": "fixture", "connection": "fixture", "vendor": "fixture", "operation": "op", "segment": "fixture",
    "fields": [{"side": "request", "path": "/value", "baseline": ['{"value":1}'] * 128,
                "live": ['{"value":100}'] * 128, "enumeration": True}],
}
sdk = Agindow(mode="repair", selection=selection)
if not sdk.ready:
    raise RuntimeError(f"shared guest unavailable: {sdk.load_error}")
calls = 0


@sdk.guard(tool="op", operation="write")
def upstream(value: dict[str, int]) -> dict[str, int]:
    global calls
    calls += 1
    return value


original = {"value": 100}
try:
    upstream(original)
    raise AssertionError("fixture should halt before dispatch")
except AgindowDriftError as error:
    print(json.dumps({"code": error.code, "phase": error.phase, "dispatches": calls}))
with sdk.context(mode="observe"):
    assert upstream(original) is original
print(json.dumps({"observe_preserved_reference": True, "total_dispatches": calls}))
sdk.close()
