"""Off-path reviewed authority, independently checked against preserved bytes."""
from __future__ import annotations
from dataclasses import dataclass, replace
import json
import re
import time
from typing import TYPE_CHECKING, Literal, cast
from .types import AgindowSetupError, Phase
from .rename_preservation import parent_parts
if TYPE_CHECKING:
    from .selective import RenameRule

RepairValueType = Literal['string', 'number', 'boolean', 'null', 'object', 'array']


@dataclass(frozen=True)
class RepairConstraint:
    key: str
    type: RepairValueType


@dataclass(frozen=True)
class RepairFacts:
    version: int
    recipe: str
    tool: str
    side: Phase
    contract: str
    migration: str
    provenance: str
    expires_at: int
    parents: tuple[str, ...]
    old: RepairConstraint
    new: RepairConstraint


def fact_label(value: object) -> bool:
    return isinstance(value, str) and re.fullmatch(r'[^\x00-\x1f\x7f]{1,256}', value) is not None


def admit_facts(facts: RepairFacts, rule: RenameRule) -> RepairFacts:
    if (not isinstance(facts, RepairFacts) or type(facts.version) is not int or facts.version != 1 or facts.recipe != 'rename'
            or facts.tool != rule.tool or facts.side != rule.side or facts.migration != rule.migration
            or not fact_label(facts.contract) or not fact_label(facts.provenance)
            or type(facts.expires_at) is not int or not 0 < facts.expires_at <= 9007199254740991
            or not isinstance(facts.parents, tuple) or facts.parents != (rule.parents if rule.parents is not None else ('',))
            or (rule.contract is not None and rule.contract != facts.contract)
            or (rule.provenance is not None and rule.provenance != facts.provenance)
            or (rule.expires_at is not None and rule.expires_at != facts.expires_at)
            or not isinstance(facts.old, RepairConstraint) or not isinstance(facts.new, RepairConstraint)
            or facts.old.key != rule.source or facts.new.key != rule.target
            or facts.old.type not in ('string', 'number', 'boolean', 'null', 'object', 'array')
            or facts.old.type != facts.new.type):
        raise AgindowSetupError('invalid selective_repair facts: require versioned, scoped, independent reviewed constraints')
    return replace(facts, parents=tuple(facts.parents), old=replace(facts.old), new=replace(facts.new))


def facts_match(before: str, after: str, facts: RepairFacts) -> bool:
    # The bounded token walk runs first, rejecting duplicates and incomplete JSON.
    if time.time() * 1000 >= facts.expires_at:
        return False
    for index, text in enumerate((before, after)):
        document: object = json.loads(text)
        for pointer in facts.parents:
            value: object = document
            for part in parent_parts(pointer):
                if isinstance(value, dict) and part in value:
                    value = cast(dict[str, object], value)[part]
                elif isinstance(value, list) and part.isascii() and part.isdigit() and str(int(part)) == part and int(part) < len(value):
                    value = cast(list[object], value)[int(part)]
                else:
                    return False
            if not isinstance(value, dict):
                return False
            constraint = facts.old if index == 0 else facts.new
            absent = facts.new.key if index == 0 else facts.old.key
            if constraint.key not in value or absent in value:
                return False
            member: object = value[constraint.key]
            kind = {str: 'string', int: 'number', float: 'number', bool: 'boolean', type(None): 'null', dict: 'object', list: 'array'}.get(type(member))
            if kind != constraint.type:
                return False
    return True
