"""Explicit reviewed field renames. Preparation is shared Rust; verification is local."""
from __future__ import annotations
from dataclasses import dataclass, replace
import json
import math
import re
from typing import Callable, cast
from .capture import capture
from .types import AgindowSetupError, Engine, EngineCall, Mode, Operation, Phase, Verdict, VerifiedRepair


@dataclass(frozen=True)
class RenameRule:
    tool: str
    side: Phase
    source: str
    target: str
    migration: str
    confidence: float


@dataclass(frozen=True)
class RepairCheck:
    before: str
    after: str
    rule: RenameRule


@dataclass(frozen=True)
class SelectiveRepair:
    rules: tuple[RenameRule, ...]
    verify: Callable[[RepairCheck], bool]


def validate_repair(value: SelectiveRepair | None) -> SelectiveRepair | None:
    if value is None:
        return None
    def fail() -> None:
        raise AgindowSetupError('invalid selective_repair: supply 1–16 scoped reviewed rename rules and a synchronous verifier')
    if not isinstance(value, SelectiveRepair) or not isinstance(value.rules, tuple) or not 1 <= len(value.rules) <= 16 or not callable(value.verify):
        fail()
    scopes: set[tuple[str, Phase]] = set()
    for rule in value.rules:
        if (not isinstance(rule, RenameRule) or rule.side not in ('request', 'response')
                or not isinstance(rule.tool, str) or re.fullmatch(r'[^\x00-\x1f\x7f]{1,256}', rule.tool) is None
                or not isinstance(rule.migration, str) or re.fullmatch(r'[^\x00-\x1f\x7f]{1,256}', rule.migration) is None
                or not isinstance(rule.source, str) or re.fullmatch(r'[A-Za-z_][A-Za-z_0-9]{0,63}', rule.source) is None
                or not isinstance(rule.target, str) or re.fullmatch(r'[A-Za-z_][A-Za-z_0-9]{0,63}', rule.target) is None
                or rule.source == rule.target or type(rule.confidence) not in (int, float)
                or not math.isfinite(rule.confidence) or not 0 <= rule.confidence <= 1
                or (rule.tool, rule.side) in scopes):
            fail()
        scopes.add((rule.tool, rule.side))
    return SelectiveRepair(tuple(value.rules), value.verify)


class SelectiveEngine:
    def __init__(self, base: Engine, options: SelectiveRepair, prepare: Callable[[str, RenameRule, Mode], str]) -> None:
        self.base, self.options, self.prepare = base, options, prepare

    def apply(self, value: object, verdict: Verdict, tool: str, side: Phase, mode: Mode) -> Verdict:
        rule = next((rule for rule in self.options.rules if rule.tool == tool and rule.side == side), None)
        if rule is None or verdict.halt or verdict.bypassed or verdict.findings or verdict.malformed or type(value) is not dict:
            return verdict
        before = capture(value)
        raw: object = json.loads(self.prepare(before, rule, mode))
        if not isinstance(raw, dict):
            raise ValueError('invalid repair preparation')
        data = cast(dict[str, object], raw)
        if type(data.get('matched')) is not bool or type(data.get('applied')) is not bool:
            raise ValueError('invalid repair preparation')
        if not data['matched']:
            return verdict
        if type(data.get('halt')) is not bool:
            raise ValueError('invalid repair policy')
        found = replace(verdict, drift_class='shape', findings=1, confidence=rule.confidence, halt=cast(bool, data['halt']))
        if not data['applied'] or mode == 'observe':
            return found
        after = data.get('body')
        if not isinstance(after, str):
            raise ValueError('invalid repair bytes')
        verified = self.options.verify(RepairCheck(before, after, rule))
        if type(verified) is not bool:
            # Close accidental coroutine objects without scheduling their work.
            close = getattr(verified, 'close', None)
            if callable(close):
                close()
            raise ValueError('repair verifier must return a synchronous boolean')
        if not verified:
            return found
        replacement: object = json.loads(after)
        return replace(found, halt=False, repair=VerifiedRepair(replacement, ((rule.source, rule.target),)))

    def begin(self, value: object, mode: Mode, operation: Operation, fail_closed: bool, tool: str) -> EngineCall:
        call = self.base.begin(value, mode, operation, fail_closed, tool)
        try:
            request = self.apply(value, call.request, tool, 'request', mode)
            return SelectiveCall(self, call, request, tool, mode)
        except Exception:
            call.close()
            raise


class SelectiveCall:
    def __init__(self, engine: SelectiveEngine, call: EngineCall, request: Verdict, tool: str, mode: Mode) -> None:
        self.engine, self.call, self.request, self.tool, self.mode = engine, call, request, tool, mode

    def finish(self, value: object) -> Verdict:
        return self.engine.apply(value, self.call.finish(value), self.tool, 'response', self.mode)

    def close(self) -> None:
        self.call.close()
