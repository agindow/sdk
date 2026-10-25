"""Bounded detached inspection projections; upstream objects are never replaced."""
from __future__ import annotations

import json
from typing import Protocol, cast, runtime_checkable

CAP = 1024 * 1024


@runtime_checkable
class Model(Protocol):
    def model_dump(self, *, mode: str) -> dict[str, object]: ...


def capture(value: object) -> str:
    count = 0
    scalars = 0
    seen: set[int] = set()

    def project(item: object, depth: int = 0) -> object:
        nonlocal count, scalars
        count += 1
        if depth > 128 or count > 100_000:
            raise ValueError("inspection limit")
        if item is None or type(item) in (str, int, float, bool):
            if isinstance(item, str):
                scalars += len(item)
                if scalars > CAP:
                    raise ValueError("inspection scalar cap")
            return item
        if type(item) not in (dict, list, tuple) and not isinstance(item, Model):
            raise ValueError("opaque inspection input")
        if id(item) in seen:
            raise ValueError("cyclic inspection input")
        seen.add(id(item))
        try:
            if isinstance(item, Model):
                return project(item.model_dump(mode="json"), depth + 1)
            if type(item) is dict:
                entries = cast(dict[object, object], item)
                if not all(type(key) is str for key in entries):
                    raise ValueError("non-string key")
                scalars += sum(len(cast(str, key)) for key in entries)
                if scalars > CAP:
                    raise ValueError("inspection key cap")
                return {cast(str, key): project(entry, depth + 1) for key, entry in entries.items()}
            return [project(entry, depth + 1) for entry in cast(tuple[object, ...] | list[object], item)]
        finally:
            seen.remove(id(item))

    result = json.dumps(project(value), allow_nan=False, separators=(",", ":"), ensure_ascii=False)
    if len(result.encode("utf-8")) > CAP:
        raise ValueError("inspection cap")
    return result
