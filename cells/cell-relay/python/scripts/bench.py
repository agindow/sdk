"""Paired local default-engine agent loop; no model/vendor/network latency."""
from __future__ import annotations

import json
import math
import platform
import sys
from importlib.metadata import version
from pathlib import Path
from statistics import mean, pstdev
from time import perf_counter_ns
from typing import Callable
from agindow import Agindow


def quantile(values: list[float], probability: float) -> float:
    return sorted(values)[math.ceil(len(values) * probability) - 1]


def measure() -> dict[str, object]:
    started = perf_counter_ns()
    sdk = Agindow()
    load_us = (perf_counter_ns() - started) / 1000
    if not sdk.ready:
        raise RuntimeError(f"guest failed to load: {sdk.load_error}")
    payload = {"result": "x" * 849}
    tools = {"lookup": lambda: payload, "verify": lambda: payload}
    guarded = {name: sdk.guard(fn) for name, fn in tools.items()}
    steps = 0
    observation = payload

    def step(target: dict[str, Callable[[], dict[str, str]]]) -> None:
        nonlocal observation, steps
        action = "lookup" if (len(observation["result"]) + steps) % 2 == 0 else "verify"
        observation = target[action]()
        if observation is not payload:
            raise RuntimeError("outcome identity lost")
        steps += 1

    for _ in range(1000):
        step(tools)
        step(guarded)
    raw: dict[str, list[float]] = {"guarded": [], "unguarded": [], "added": []}
    for index in range(5000):
        sample: dict[str, float] = {}
        for kind in (("guarded", "unguarded") if index % 2 == 0 else ("unguarded", "guarded")):
            started = perf_counter_ns()
            step(guarded if kind == "guarded" else tools)
            sample[kind] = (perf_counter_ns() - started) / 1000
            raw[kind].append(sample[kind])
        raw["added"].append(sample["guarded"] - sample["unguarded"])
    summary = {key: {"p50": quantile(values, .5), "p99": quantile(values, .99),
                     "mean": mean(values), "stddev": pstdev(values)} for key, values in raw.items()}
    bypasses = sum(event.type == "bypass" for event in sdk.recent())
    report: dict[str, object] = {"schema": "agindow.python-loop/1", "samples": 5000, "warmups": 1000,
        "fixture": "local agent tool-selection/observation loop; NullEngine through the shared release WASM",
        "estimator": "p50 of alternating paired guarded-minus-unguarded steps; nearest-rank quantiles",
        "steps": steps, "payloadBytes": 862, "bypasses": bypasses, "loadUs": load_us, "summary": summary, "raw": raw,
        "environment": {"python": sys.version, "wasmtime": version("wasmtime"), "platform": platform.platform(),
                        "arch": platform.machine(), "cpu": platform.processor()},
        "exclusions": ["module import before load timer", "model inference", "vendor/network", "statistical paths (TI-53)"]}
    output = Path(__file__).resolve().parents[4] / "target/python-loop/report.json"
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps({"addedUs": summary["added"], "loadUs": load_us, "report": str(output)}))
    if summary["added"]["p50"] >= 100 or bypasses:
        raise RuntimeError("Python default-engine loop budget failed")
    return report


if __name__ == "__main__":
    measure()
