"""Independent bounded token walk; untouched numeric lexemes never round-trip."""
from __future__ import annotations
import json
import re


def parent_parts(pointer: str) -> tuple[str, ...]:
    if len(pointer.encode('utf-8')) > 512 or (pointer != '' and not pointer.startswith('/')) or re.search(r'~(?![01])', pointer):
        raise ValueError('invalid parent pointer')
    parts = () if pointer == '' else tuple(p.replace('~1', '/').replace('~0', '~') for p in pointer[1:].split('/'))
    if len(parts) > 32:
        raise ValueError('parent depth limit')
    return parts


def selected_parents(parents: tuple[str, ...], source: str) -> None:
    if not 1 <= len(parents) <= 16:
        raise ValueError('parent count limit')
    paths = [(*parent_parts(p), source) for p in parents]
    for i, path in enumerate(paths):
        if any(path[:len(other)] == other or other[:len(path)] == path for other in paths[i + 1:]):
            raise ValueError('overlapping selections')


def escape(key: str) -> str:
    return re.sub(r'([\\.\[\]$])', r'\\\1', key) if key else r'\e'


def rename_plan(before: str, source: str, target: str, parents: tuple[str, ...]) -> tuple[str, tuple[tuple[str, str], ...]]:
    if len(before.encode('utf-8')) > 1_048_576:
        raise ValueError('capture limit')
    selected = {parent_parts(p) for p in parents}
    edits: list[tuple[int, int, str, str]] = []
    cursor, nodes = 0, 0
    def peek() -> str:
        return before[cursor:cursor + 1]
    def whitespace() -> None:
        nonlocal cursor
        while peek() and peek() in ' \r\n\t':
            cursor += 1
    def string() -> str:
        nonlocal cursor
        start = cursor
        if peek() != '"':
            raise ValueError('expected string')
        cursor += 1
        while cursor < len(before):
            c = peek()
            cursor += 1
            if c == '\\':
                cursor += 1
            elif c == '"':
                value: object = json.loads(before[start:cursor])
                if not isinstance(value, str):
                    raise ValueError('invalid string')
                return value
        raise ValueError('incomplete string')
    scalar = re.compile(r'(?:null|true|false|-?(?:0|[1-9][0-9]*)(?:\.[0-9]+)?(?:[eE][+-]?[0-9]+)?)')
    def visit(parts: tuple[str, ...], path: str, depth: int) -> None:
        nonlocal cursor, nodes
        nodes += 1
        if nodes > 4096 or depth > 64:
            raise ValueError('inspection limit')
        whitespace()
        if peek() == '{':
            cursor += 1
            whitespace()
            keys: set[str] = set()
            active = parts in selected
            if peek() != '}':
                while True:
                    whitespace()
                    start = cursor
                    key = string()
                    end = cursor
                    if key in keys:
                        raise ValueError('duplicate member')
                    keys.add(key)
                    whitespace()
                    if peek() != ':':
                        raise ValueError('expected colon')
                    cursor += 1
                    child = escape(key) if path == '$' else f'{path}.{escape(key)}'
                    if active and key == source:
                        edits.append((start, end, child, escape(target) if path == '$' else f'{path}.{escape(target)}'))
                    visit((*parts, key), child, depth + 1)
                    whitespace()
                    if peek() != ',':
                        break
                    cursor += 1
            if peek() != '}':
                raise ValueError('expected object end')
            cursor += 1
            if active and (source not in keys or target in keys):
                raise ValueError('missing member or collision')
        elif peek() == '[':
            cursor += 1
            whitespace()
            index = 0
            if peek() != ']':
                while True:
                    visit((*parts, str(index)), f'{path}[{index}]', depth + 1)
                    index += 1
                    whitespace()
                    if peek() != ',':
                        break
                    cursor += 1
            if peek() != ']':
                raise ValueError('expected array end')
            cursor += 1
        elif peek() == '"':
            string()
        else:
            token = scalar.match(before, cursor)
            if token is None:
                raise ValueError('invalid scalar')
            cursor += len(token[0])
    visit((), '$', 0)
    whitespace()
    if cursor != len(before) or len(edits) != len(selected):
        raise ValueError('incomplete selection')
    after = before
    for start, end, _, _ in reversed(edits):
        after = after[:start] + json.dumps(target, ensure_ascii=False) + after[end:]
    return after, tuple((path, to) for _, _, path, to in edits)
