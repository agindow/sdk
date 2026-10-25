"""Execute the shared wrapper corpus against the actual Python public API."""
from __future__ import annotations

import asyncio
import inspect
import json
import sys
from typing import Awaitable, TypedDict, cast
from agindow import Agindow, AgindowDriftError, Config, Mode, Operation, Verdict
from agindow.types import EngineCall


class Fixture(TypedDict):
    id: str
    scenario: str
    mode: Mode
    operation: Operation
    args: object
    recorded: object


class Definition(TypedDict):
    version: int
    cases: list[Fixture]


PASS = Verdict()
HALT = Verdict(halt=True, findings=1, drift_class="semantic")


class Call:
    def __init__(self, scenario: str) -> None:
        self.scenario = scenario
        self.request = HALT if scenario == "halt_request" else PASS

    def finish(self, value: object) -> Verdict:
        return HALT if self.scenario == "halt_response" else PASS

    def close(self) -> None:
        pass


class ScriptedEngine:
    def __init__(self, scenario: str) -> None:
        self.scenario = scenario

    def begin(self, value: object, mode: Mode, operation: Operation, fail_closed: bool, tool: str) -> EngineCall:
        if self.scenario == "fault":
            raise RuntimeError("fixture")
        return Call(self.scenario)


def selection(side: str) -> dict[str, object]:
    return {"tenant": "t", "connection": "c", "vendor": "v", "operation": "op", "segment": "s",
            "fields": [{"side": side, "path": "/value", "baseline": ['{"value":1}'] * 128,
                        "live": ['{"value":100}'] * 128, "enumeration": True}]}


async def resolve(value: Awaitable[object]) -> object:
    return await value


def run(fixture: Fixture) -> dict[str, object]:
    scenario = fixture["scenario"]
    config = Config(mode=fixture["mode"])
    if scenario.startswith("wasm_"):
        sdk = Agindow(config, selection=selection("response" if scenario == "wasm_response" else "request"),
                      work=0 if scenario == "wasm_budget" else 200_000_000)
        if not sdk.ready:
            raise RuntimeError(f"real guest not ready: {sdk.load_error}")
    else:
        sdk = Agindow(config, engine=ScriptedEngine(scenario))
    calls = 0
    same_args = same_this = same_value = same_error = retained_outcome = is_async = False
    code: str | None = None
    drift_class: str | None = None
    original_error = ValueError("original fixture exception")

    async def completed() -> object:
        return fixture["recorded"]

    class Receiver:
        def invoke(self, value: object) -> object:
            nonlocal calls, same_args, same_this
            calls += 1
            same_args = value is fixture["args"]
            same_this = self is receiver
            if scenario == "error":
                raise original_error
            return completed() if scenario == "async" else fixture["recorded"]

    receiver = Receiver()
    fn = sdk.guard(receiver.invoke, tool="op", operation=fixture["operation"])
    try:
        result = fn(fixture["args"])
        is_async = inspect.isawaitable(result)
        if inspect.isawaitable(result):
            result = asyncio.run(resolve(result))
        same_value = result is fixture["recorded"]
    except BaseException as error:
        same_error = error is original_error
        if isinstance(error, AgindowDriftError):
            code = error.code
            drift_class = error.drift_class
            retained_outcome = error.outcome is fixture["recorded"]
        elif error is not original_error:
            raise
    rows: dict[str, object] = {"id": fixture["id"], "calls": calls, "sameValue": same_value,
        "sameError": same_error, "retainedOutcome": retained_outcome, "code": code, "driftClass": drift_class,
        "sameArgs": same_args, "sameThis": same_this, "isAsync": is_async,
        "bypasses": sum(event.type == "bypass" for event in sdk.recent())}
    sdk.close()
    return rows


if __name__ == "__main__":
    definition = cast(Definition, json.load(sys.stdin))
    print(json.dumps([run(fixture) for fixture in definition["cases"]]))
