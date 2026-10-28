/** Executed verbatim against the staged Python distribution by the docs gate. */
export const detectionPython = `import json
from agindow import Agindow, Event

# Labelled local fixture: numeric magnitude shift, not live vendor traffic.
def selection_for(value: int, count: int = 128) -> dict[str, object]:
    return {
        "tenant": "demo", "connection": "local", "vendor": "synthetic",
        "operation": "demo.detect", "segment": "default",
        "fields": [{"side": "request", "path": "/value", "enumeration": True,
                    "baseline": [json.dumps({"value": 1}) for _ in range(count)],
                    "live": [json.dumps({"value": value}) for _ in range(count)]}],
    }


def observe(selection: dict[str, object] | None, value: int = 100) -> tuple[Event, ...]:
    sdk = Agindow(mode="observe", selection=selection)
    try:
        assert sdk.ready, sdk.load_error
        payload = {"value": value}

        @sdk.guard(tool="demo.detect", operation="read")
        def read(body: dict[str, int]) -> dict[str, int]:
            return body

        assert read(payload) is payload
        events = sdk.recent()
        assert not any(event.type in ("bypass", "repair", "blocked") for event in events)
        return events
    finally:
        sdk.close()


changed = observe(selection_for(100))
drift = next(event for event in changed if event.type == "drift")
verdict = drift.verdict
assert verdict is not None
assert verdict.findings == 1 and verdict.detectors == 5
assert verdict.drift_class == "semantic"
assert not verdict.bypassed and not verdict.halt and verdict.repair is None
assert not any(event.type == "drift" for event in observe(selection_for(1), 1))
assert not any(event.type == "drift" for event in observe(None))
assert not any(event.type == "drift" for event in observe(selection_for(100, 4)))
print(json.dumps({"event": drift.type, "findings": verdict.findings,
                  "detectors": verdict.detectors, "class": verdict.drift_class,
                  "bypassed": verdict.bypassed, "halt": verdict.halt,
                  "repairApplied": verdict.repair is not None,
                  "resultUnchanged": True}, indent=2))
`;
