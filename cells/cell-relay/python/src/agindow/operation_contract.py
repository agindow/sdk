"""Explicit advisory operation comparisons through the packaged shared guest."""
from __future__ import annotations
from dataclasses import dataclass, asdict
import json
from typing import Literal, cast
from .types import Phase


@dataclass(frozen=True)
class OperationScope:
    route: str
    method: Literal['GET', 'POST', 'PUT', 'PATCH', 'DELETE', 'HEAD', 'OPTIONS']
    side: Phase
    version: str


@dataclass(frozen=True)
class OperationSource:
    scope: OperationScope
    kind: Literal['declared', 'observed']
    provenance: str
    body: str
    complete: bool


def compare_operation(baseline: OperationSource, current: OperationSource, work: int = 200_000_000) -> dict[str, object]:
    from .wasm import WasmEngine
    try:
        serialized = json.dumps({'baseline': asdict(baseline), 'current': asdict(current)}, ensure_ascii=False)
        if len(serialized.encode('utf-8')) > 2_200_000:
            raise ValueError('capture limit')
        guest = WasmEngine(work=work)
        result: object = json.loads(guest.compare_operation(serialized))
        if not isinstance(result, dict) or result.get('coverage') not in ('complete', 'partial', 'unsupported') or not isinstance(result.get('findings'), list):
            raise ValueError('invalid comparison result')
        return cast(dict[str, object], result)
    except Exception:
        return {'coverage': 'unsupported', 'gaps': ['comparison-unavailable'], 'requiredness': 'unknown', 'findings': []}
