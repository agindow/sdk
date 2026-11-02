"""Explicit reviewed field renames. Preparation is shared Rust; verification is local."""
from __future__ import annotations
from dataclasses import dataclass, replace
import json
import math
import re
import time
from typing import Callable, cast
from .capture import capture
from .scalar import ScalarRule, admit_scalar, scalar_plan, fixed
from .scalar_tokens import scalar_token
from .repair_facts import RepairFacts, admit_facts, fact_label, facts_match
from .rename_preservation import rename_plan, selected_parents
from .types import AgindowSetupError, Engine, EngineCall, Mode, Operation, Phase, Verdict, VerifiedRepair


@dataclass(frozen=True)
class RenameRule:
    tool: str
    side: Phase
    source: str
    target: str
    migration: str
    confidence: float
    parents: tuple[str, ...] | None = None
    contract: str | None = None
    provenance: str | None = None
    expires_at: int | None = None
    facts: RepairFacts | None = None


@dataclass(frozen=True)
class RepairCheck:
    before: str
    after: str
    rule: RenameRule | ScalarRule


@dataclass(frozen=True)
class SelectiveRepair:
    rules: tuple[RenameRule | ScalarRule, ...]
    verify: Callable[[RepairCheck], bool]


def validate_repair(value: SelectiveRepair | None) -> SelectiveRepair | None:
    if value is None:
        return None
    def fail() -> None:
        raise AgindowSetupError('invalid selective_repair: supply 1–16 scoped reviewed rename rules and a synchronous verifier')
    if not isinstance(value, SelectiveRepair) or not isinstance(value.rules, tuple) or not 1 <= len(value.rules) <= 16 or not callable(value.verify):
        fail()
    scopes: set[tuple[str, Phase]] = set()
    rules: list[RenameRule | ScalarRule] = []
    for rule in value.rules:
        if isinstance(rule, ScalarRule):
            admitted = admit_scalar(rule)
            if (rule.tool, rule.side) in scopes:
                fail()
            scopes.add((rule.tool, rule.side))
            rules.append(admitted)
            continue
        if not isinstance(rule, RenameRule):
            fail()
        if rule.parents is not None:
            try:
                if (not isinstance(rule.parents, tuple) or any(not isinstance(p, str) for p in rule.parents)
                        or not isinstance(rule.contract, str) or re.fullmatch(r'[^\x00-\x1f\x7f]{1,256}', rule.contract) is None
                        or not isinstance(rule.provenance, str) or re.fullmatch(r'[^\x00-\x1f\x7f]{1,256}', rule.provenance) is None
                        or type(rule.expires_at) is not int or not 0 < rule.expires_at <= 9007199254740991):
                    fail()
                selected_parents(rule.parents, rule.source)
            except (ValueError, TypeError):
                fail()
        def key(value: str) -> bool:
            if rule.parents is None:
                return re.fullmatch(r'[A-Za-z_][A-Za-z_0-9]{0,63}', value) is not None
            return re.search(r'[\x00-\x1f\x7f\ud800-\udfff]', value) is None and len(value.encode('utf-8')) <= 64
        if (not isinstance(rule, RenameRule) or rule.side not in ('request', 'response')
                or not isinstance(rule.tool, str) or re.fullmatch(r'[^\x00-\x1f\x7f]{1,256}', rule.tool) is None
                or not isinstance(rule.migration, str) or re.fullmatch(r'[^\x00-\x1f\x7f]{1,256}', rule.migration) is None
                or not isinstance(rule.source, str) or not key(rule.source)
                or not isinstance(rule.target, str) or not key(rule.target)
                or rule.source == rule.target or type(rule.confidence) not in (int, float)
                or not math.isfinite(rule.confidence) or not 0 <= rule.confidence <= 1
                or (rule.tool, rule.side) in scopes):
            fail()
        if ((rule.contract is not None and not fact_label(rule.contract))
                or (rule.provenance is not None and not fact_label(rule.provenance))
                or (rule.expires_at is not None and (type(rule.expires_at) is not int or not 0 < rule.expires_at <= 9007199254740991))):
            fail()
        facts = None if rule.facts is None else admit_facts(rule.facts, rule)
        rules.append(replace(rule, confidence=float(rule.confidence), facts=facts))
        scopes.add((rule.tool, rule.side))
    return SelectiveRepair(tuple(rules), value.verify)


class SelectiveEngine:
    def __init__(self, base: Engine, options: SelectiveRepair, prepare: Callable[[str, RenameRule | ScalarRule, Mode], str]) -> None:
        self.base, self.options, self.prepare = base, options, prepare

    def apply(self, value: object, verdict: Verdict, tool: str, side: Phase, mode: Mode, operation: Operation) -> Verdict:
        rule = next((rule for rule in self.options.rules if rule.tool == tool and rule.side == side), None)
        if rule is None or verdict.halt or verdict.bypassed or verdict.findings or verdict.malformed or type(value) not in (dict, list):
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
        scalar = isinstance(rule, ScalarRule)
        expires_at = rule.facts.expires_at if isinstance(rule, ScalarRule) else rule.expires_at
        found = replace(verdict, drift_class='semantic' if scalar else 'shape', findings=1, confidence=rule.confidence, halt=(mode != 'observe' and operation != 'read') if scalar else cast(bool, data['halt']),
                        repair_refusal=str(data.get('reason', 'unavailable-candidate')) if scalar and not data['applied'] else None)
        if scalar and rule.confidence < .99:
            return replace(found, repair_refusal='confidence-or-policy')
        if not data['applied'] or mode == 'observe':
            return found
        if expires_at is not None and time.time() * 1000 >= expires_at:
            return replace(found, repair_refusal='stale-facts') if scalar else found
        after = data.get('body')
        if not isinstance(after, str):
            raise ValueError('invalid repair bytes')
        expected, changes = scalar_plan(before, rule.facts) if isinstance(rule, ScalarRule) else rename_plan(before, rule.source, rule.target, rule.parents or ('',))
        if expected != after:
            raise ValueError('repair preservation verification failed')
        if isinstance(rule, RenameRule) and rule.facts is not None and not facts_match(before, after, rule.facts):
            return found
        verified = self.options.verify(RepairCheck(before, after, rule))
        if type(verified) is not bool:
            # Close accidental coroutine objects without scheduling their work.
            close = getattr(verified, 'close', None)
            if callable(close):
                close()
            raise ValueError('repair verifier must return a synchronous boolean')
        if (not verified or (rule.facts is not None and time.time() * 1000 >= rule.facts.expires_at)
                or (expires_at is not None and time.time() * 1000 >= expires_at)):
            return replace(found, repair_refusal='verifier-refused' if not verified else 'stale-facts') if scalar else found
        replacement: object = json.loads(after)
        if isinstance(rule, ScalarRule) and rule.facts.recipe == 'scale':
            projected = capture(replacement)
            start, end, _ = scalar_token(projected, rule.facts.pointer)
            original_start, original_end, _ = scalar_token(after, rule.facts.pointer)
            if fixed(projected[start:end]) != fixed(after[original_start:original_end]):
                return replace(found, repair_refusal='projection-precision')
        return replace(found, halt=False, repair=VerifiedRepair(replacement, changes))

    def begin(self, value: object, mode: Mode, operation: Operation, fail_closed: bool, tool: str) -> EngineCall:
        call = self.base.begin(value, mode, operation, fail_closed, tool)
        try:
            request = self.apply(value, call.request, tool, 'request', mode, operation)
            return SelectiveCall(self, call, request, tool, mode, operation)
        except Exception:
            call.close()
            raise


class SelectiveCall:
    def __init__(self, engine: SelectiveEngine, call: EngineCall, request: Verdict, tool: str, mode: Mode, operation: Operation) -> None:
        self.engine, self.call, self.request, self.tool, self.mode, self.operation = engine, call, request, tool, mode, operation

    def finish(self, value: object) -> Verdict:
        return self.engine.apply(value, self.call.finish(value), self.tool, 'response', self.mode, self.operation)

    def close(self) -> None:
        self.call.close()
