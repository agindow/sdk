import { parentParts } from './rename-preservation.js';
/** Independent bounded walk over the original bytes; duplicate keys never collapse. */
export function scalarToken(before: string, pointer: string): {start: number; end: number; path: string; raw: string} {
  if (Buffer.byteLength(before) > 1_048_576 || pointer === '') throw Error('scalar capture/pointer limit');
  const selection = JSON.stringify(parentParts(pointer));
  let cursor = 0; let nodes = 0;
  let found: {start: number; end: number; path: string; raw: string} | undefined;
  const space = (): void => { while (/[ \r\n\t]/u.test(before[cursor] ?? '\0')) cursor++; };
  const string = (): string => {
    const start = cursor; if (before[cursor++] !== '"') throw Error('expected string');
    while (cursor < before.length) {
      const c = before[cursor++];
      if (c === '\\') cursor++;
      else if (c === '"') {
        const value: unknown = JSON.parse(before.slice(start, cursor));
        if (typeof value !== 'string' || /[\uD800-\uDFFF]/u.test(value)) throw Error('invalid Unicode string');
        return value;
      }
    }
    throw Error('incomplete string');
  };
  const escape = (s: string): string => s === '' ? '\\e' : s.replace(/[\\.\[\]$]/gu, '\\$&');
  function visit(parts: string[], path: string, depth: number): void {
    if (++nodes > 4096 || depth > 64) throw Error('scalar inspection limit');
    space(); const start = cursor; const c = before[cursor];
    if (c === '{') {
      cursor++; space(); const keys = new Set<string>();
      if (before[cursor] !== '}') while (true) {
        space(); const key = string(); if (keys.has(key)) throw Error('duplicate key'); keys.add(key);
        space(); if (before[cursor++] !== ':') throw Error('expected colon');
        visit([...parts, key], path === '$' ? escape(key) : `${path}.${escape(key)}`, depth + 1); space();
        if (before[cursor] !== ',') break; cursor++;
      }
      if (before[cursor++] !== '}') throw Error('expected object end');
    } else if (c === '[') {
      cursor++; space(); let index = 0;
      if (before[cursor] !== ']') while (true) {
        visit([...parts, String(index)], `${path}[${index}]`, depth + 1); index++; space();
        if (before[cursor] !== ',') break; cursor++;
      }
      if (before[cursor++] !== ']') throw Error('expected array end');
    } else if (c === '"') string();
    else {
      const match = /^(?:null|true|false|-?(?:0|[1-9][0-9]*)(?:\.[0-9]+)?(?:[eE][+-]?[0-9]+)?)/u.exec(before.slice(cursor));
      if (!match) throw Error('invalid scalar'); cursor += match[0].length;
    }
    if (selection === JSON.stringify(parts)) found = {start, end: cursor, path, raw: before.slice(start, cursor)};
  }
  visit([], '$', 0); space();
  if (cursor !== before.length || found === undefined) throw Error('incomplete scalar selection');
  return found;
}
