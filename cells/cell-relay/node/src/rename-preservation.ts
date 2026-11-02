/** Independent host token walk: numbers and untouched strings never round-trip. */
export interface RenamePlan { readonly after: string; readonly changes: readonly {path: string; to: string}[] }
export function parentParts(pointer: string): string[] {
  if (/[\uD800-\uDFFF]/u.test(pointer) || Buffer.byteLength(pointer) > 512 || (pointer !== '' && !pointer.startsWith('/')) || /~(?![01])/u.test(pointer)) throw Error('invalid parent pointer');
  const parts = pointer === '' ? [] : pointer.slice(1).split('/').map(p => p.replaceAll('~1', '/').replaceAll('~0', '~'));
  if (parts.length > 32) throw Error('parent depth limit');
  return parts;
}
export function selectedParents(parents: readonly string[], source: string): void {
  if (parents.length < 1 || parents.length > 16) throw Error('parent count limit');
  const paths = parents.map(p => [...parentParts(p), source]);
  for (const [i, path] of paths.entries()) {
    if (paths.slice(i + 1).some(other => path.every((p, j) => p === other[j]) || other.every((p, j) => p === path[j]))) throw Error('overlapping selections');
  }
}
const escape = (key: string): string => key === '' ? '\\e' : key.replace(/[\\.\[\]$]/gu, '\\$&');
export function renamePlan(before: string, source: string, target: string, parents: readonly string[]): RenamePlan {
  if (Buffer.byteLength(before) > 1_048_576) throw Error('capture limit');
  const selected = new Set(parents.map(p => JSON.stringify(parentParts(p))));
  const edits: {start: number; end: number; path: string; to: string}[] = [];
  let cursor = 0; let nodes = 0;
  const whitespace = (): void => { while (/[ \r\n\t]/u.test(before[cursor] ?? '\0')) cursor++; };
  const string = (): string => {
    const start = cursor;
    if (before[cursor++] !== '"') throw Error('expected string');
    while (cursor < before.length) {
      const c = before[cursor++];
      if (c === '\\') cursor++;
      else if (c === '"') {
        const value: unknown = JSON.parse(before.slice(start, cursor));
        if (typeof value !== 'string') throw Error('invalid string');
        return value;
      }
    }
    throw Error('incomplete string');
  };
  function visit(parts: string[], path: string, depth: number): void {
    if (++nodes > 4096 || depth > 64) throw Error('inspection limit');
    whitespace();
    const c = before[cursor];
    if (c === '{') {
      cursor++; whitespace(); const keys = new Set<string>();
      const active = selected.has(JSON.stringify(parts));
      if (before[cursor] !== '}') while (true) {
        whitespace(); const start = cursor; const key = string(); const end = cursor;
        if (keys.has(key)) throw Error('duplicate member'); keys.add(key);
        whitespace(); if (before[cursor++] !== ':') throw Error('expected colon');
        const child = path === '$' ? escape(key) : `${path}.${escape(key)}`;
        if (active && key === source) edits.push({start, end, path: child, to: path === '$' ? escape(target) : `${path}.${escape(target)}`});
        visit([...parts, key], child, depth + 1); whitespace();
        if (before[cursor] !== ',') break;
        cursor++;
      }
      if (before[cursor++] !== '}') throw Error('expected object end');
      if (active && (!keys.has(source) || keys.has(target))) throw Error('missing member or collision');
    } else if (c === '[') {
      cursor++; whitespace(); let index = 0;
      if (before[cursor] !== ']') while (true) {
        visit([...parts, String(index)], `${path}[${index}]`, depth + 1); index++; whitespace();
        if (before[cursor] !== ',') break;
        cursor++;
      }
      if (before[cursor++] !== ']') throw Error('expected array end');
    } else if (c === '"') string();
    else {
      const token = /^(?:null|true|false|-?(?:0|[1-9][0-9]*)(?:\.[0-9]+)?(?:[eE][+-]?[0-9]+)?)/u.exec(before.slice(cursor));
      if (!token) throw Error('invalid scalar'); cursor += token[0].length;
    }
  }
  visit([], '$', 0); whitespace();
  if (cursor !== before.length || edits.length !== selected.size) throw Error('incomplete selection');
  let after = before;
  for (const edit of [...edits].reverse()) after = after.slice(0, edit.start) + JSON.stringify(target) + after.slice(edit.end);
  return {after, changes: edits.map(({path, to}) => ({path, to}))};
}
