"""Reviewed scalar facts and an independent exact integer arithmetic oracle."""
from __future__ import annotations
from dataclasses import dataclass, asdict, replace
import json
import math
import re
from typing import Literal
from .types import AgindowSetupError, Phase
from .rename_preservation import parent_parts
from .scalar_tokens import scalar_token


@dataclass(frozen=True)
class ScalarFacts:
    version: Literal[1]
    recipe: Literal['string-map', 'empty-null', 'scale']
    tool: str
    side: Phase
    contract: str
    migration: str
    provenance: str
    expires_at: int
    pointer: str
    mapping: tuple[tuple[str, str], ...] | None = None
    equivalence: tuple[tuple[str, str], ...] | None = None
    null_meaning: str | None = None
    source_unit: str | None = None
    target_unit: str | None = None
    numerator: str | None = None
    denominator: str | None = None
    input_min: str | None = None
    input_max: str | None = None
    output_min: str | None = None
    output_max: str | None = None


@dataclass(frozen=True)
class ScalarRule:
    tool: str
    side: Phase
    migration: str
    confidence: float
    facts: ScalarFacts


def fixed(value: str) -> int:
    if not isinstance(value, str) or len(value) > 80:
        raise ValueError('numeric bound')
    match = re.fullmatch(r'(-?)(0|[1-9][0-9]*)(?:\.([0-9]+))?(?:[eE]([+-]?[0-9]+))?', value)
    if match is None:
        raise ValueError('not a numeric token')
    sign, integer, fraction, exponent = match.groups()
    fraction, exponent = fraction or '', exponent or '0'
    exp = int(exponent)
    if abs(exp) > 18 or len(integer) + len(fraction) > 36:
        raise ValueError('numeric precision')
    power = 18 + exp - len(fraction)
    coefficient = int(sign + integer + fraction)
    if power < 0:
        divisor = 10 ** -power
        if coefficient % divisor:
            raise ValueError('numeric precision')
        coefficient //= divisor
    else:
        coefficient *= 10 ** power
    reduced, places = coefficient, 18
    while places and reduced % 10 == 0:
        reduced //= 10
        places -= 1
    if abs(reduced) >= 10 ** 36:
        raise ValueError('numeric overflow')
    return int(coefficient)


def wire(facts: ScalarFacts) -> dict[str, object]:
    def camel(key: str) -> str:
        first, *rest = key.split('_')
        return first + ''.join(word.title() for word in rest)
    return {camel(key): value for key, value in asdict(facts).items() if value is not None}


def admit_scalar(rule: ScalarRule) -> ScalarRule:
    try:
        f = rule.facts
        def label(value: object) -> bool:
            return isinstance(value, str) and len(value.encode('utf-8')) <= 256 and re.fullmatch(r'[^\x00-\x1f\x7f\ud800-\udfff]{1,256}', value) is not None
        if (not isinstance(f, ScalarFacts) or type(f.version) is not int or f.version != 1 or f.tool != rule.tool or f.side != rule.side or f.migration != rule.migration
                or not all(label(v) for v in (rule.tool, rule.migration, f.contract, f.provenance)) or f.side not in ('request', 'response')
                or type(f.expires_at) is not int or not 0 < f.expires_at <= 9007199254740991 or not f.pointer
                or type(rule.confidence) not in (float, int) or not math.isfinite(rule.confidence) or not 0 <= rule.confidence <= 1):
            raise ValueError('scope')
        parent_parts(f.pointer)
        numeric = (f.source_unit, f.target_unit, f.numerator, f.denominator, f.input_min, f.input_max, f.output_min, f.output_max)
        if f.recipe == 'string-map' and all(v is None for v in numeric) and f.null_meaning is None:
            if not isinstance(f.mapping, tuple) or not 1 <= len(f.mapping) <= 64:
                raise ValueError('mapping bound')
            sources: set[str] = set()
            for pair in f.mapping:
                if not isinstance(pair, tuple) or len(pair) != 2:
                    raise ValueError('mapping pair')
                source, target = pair
                if (not isinstance(source, str) or not isinstance(target, str) or len(source.encode()) > 256 or len(target.encode()) > 256
                        or source == target or source in sources):
                    raise ValueError('mapping ambiguity')
                sources.add(source)
            reviews: dict[str, str] = {}
            if f.equivalence is not None:
                if not isinstance(f.equivalence, tuple) or len(f.equivalence) > 64:
                    raise ValueError('equivalence bound')
                for pair in f.equivalence:
                    if not isinstance(pair, tuple) or len(pair) != 2 or pair[0] not in sources or not label(pair[1]) or pair[0] in reviews:
                        raise ValueError('equivalence review')
                    reviews[pair[0]] = pair[1]
            for source, target in f.mapping:
                if sum(t == target for _, t in f.mapping) > 1 and source not in reviews:
                    raise ValueError('missing source equivalence review')
        elif f.recipe == 'empty-null' and all(v is None for v in numeric) and f.mapping is None and f.equivalence is None and label(f.null_meaning):
            pass
        elif f.recipe == 'scale' and f.mapping is None and f.equivalence is None and f.null_meaning is None:
            ratios = {('cents', 'dollars'): ('1', '100'), ('dollars', 'cents'): ('100', '1'),
                      ('seconds', 'milliseconds'): ('1000', '1'), ('milliseconds', 'seconds'): ('1', '1000'),
                      ('meters', 'millimeters'): ('1000', '1'), ('millimeters', 'meters'): ('1', '1000')}
            if ratios.get((f.source_unit or '', f.target_unit or '')) != (f.numerator, f.denominator):
                raise ValueError('units/ratio')
            if fixed(f.input_min or '') > fixed(f.input_max or '') or fixed(f.output_min or '') > fixed(f.output_max or ''):
                raise ValueError('domain bounds')
        else:
            raise ValueError('unsupported recipe')
        return replace(rule, confidence=float(rule.confidence))
    except (ValueError, TypeError, AttributeError):
        raise AgindowSetupError('invalid scalar repair: require reviewed scoped exact facts') from None


def scalar_plan(before: str, facts: ScalarFacts) -> tuple[str, tuple[tuple[str, str], ...]]:
    start, end, path = scalar_token(before, facts.pointer)
    raw = before[start:end]
    if facts.recipe == 'string-map':
        source: object = json.loads(raw)
        target = next((target for old, target in facts.mapping or () if old == source), None)
        if not isinstance(source, str) or target is None:
            raise ValueError('unknown source')
        # Match serde's UTF-8 string token spelling.
        replacement = json.dumps(target, ensure_ascii=False, separators=(',', ':')).replace('\\u0008', '\\b').replace('\\u000c', '\\f')
    elif facts.recipe == 'empty-null':
        if raw != '""':
            raise ValueError('not actually empty')
        replacement = 'null'
    else:
        value = fixed(raw)
        n, d = int(facts.numerator or ''), int(facts.denominator or '')
        product = value * n
        if d <= 0 or product % d:
            raise ValueError('inexact decimal')
        output = product // d
        digits = str(abs(output)).zfill(19)
        replacement = ('-' if output < 0 else '') + (digits[:-18] + '.' + digits[-18:]).rstrip('0').rstrip('.')
        fixed(replacement)
        if not fixed(facts.input_min or '') <= value <= fixed(facts.input_max or '') or not fixed(facts.output_min or '') <= output <= fixed(facts.output_max or ''):
            raise ValueError('domain bounds')
    return before[:start] + replacement + before[end:], ((path, path),)
