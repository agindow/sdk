/**
 * The frozen-contract differ (TI-02).
 *
 * A released `vN` schema is immutable except for its annotations. That is
 * stricter than the usual “breaking changes only” reading, and deliberately
 * so: our validators are closed (`additionalProperties: false`), so *any*
 * semantic delta — an added optional field included — is rejected by an
 * already-deployed consumer. A new version is the only safe path, and this
 * function is what tells a human that with a line they can read.
 *
 * Pure functions only: no filesystem, no process — usable from a test helper,
 * a CI job or a future control-plane check alike.
 */

/** Keys that describe a schema rather than constrain a message. */
const ANNOTATIONS = new Set([
  'title',
  'description',
  '$comment',
  'examples',
  'default',
  '$id',
  '$schema',
  'deprecated',
  'readOnly',
  'writeOnly',
]);

/**
 * Every semantic difference between a frozen baseline and the current schema,
 * as readable lines (`-` removed, `+` added, `~` changed). Empty means the
 * edit is annotation-only.
 */
export function semanticChanges(baseline: unknown, current: unknown): string[] {
  return diff(normalize(baseline), normalize(current), '$');
}

/** Drops annotations and sorts keys, so two equal schemas compare equal. */
export function normalize(schema: unknown): unknown {
  if (Array.isArray(schema)) return schema.map(normalize);
  if (isRecord(schema)) {
    const out: Record<string, unknown> = {};
    for (const key of Object.keys(schema).sort()) {
      if (ANNOTATIONS.has(key)) continue;
      out[key] = normalize(schema[key]);
    }
    return out;
  }
  return schema;
}

function diff(a: unknown, b: unknown, path: string): string[] {
  if (JSON.stringify(a) === JSON.stringify(b)) return [];

  if (isRecord(a) && isRecord(b)) {
    const lines: string[] = [];
    for (const key of unionKeys(a, b)) {
      const childPath = `${path}.${key}`;
      const inA = Object.hasOwn(a, key);
      const inB = Object.hasOwn(b, key);
      if (inA && !inB) lines.push(`- ${childPath} = ${render(a[key])}`);
      else if (!inA && inB) lines.push(`+ ${childPath} = ${render(b[key])}`);
      else lines.push(...diff(a[key], b[key], childPath));
    }
    return lines;
  }

  if (Array.isArray(a) && Array.isArray(b)) return diffArrays(a, b, path);

  return [`~ ${path}: ${render(a)} → ${render(b)}`];
}

/** Array elements are compared as members, not positions — `required` reads best that way. */
function diffArrays(a: readonly unknown[], b: readonly unknown[], path: string): string[] {
  const lines: string[] = [];
  for (const item of a) {
    if (!b.some((other) => JSON.stringify(item) === JSON.stringify(other))) {
      lines.push(`- ${path}[] = ${render(item)}`);
    }
  }
  for (const item of b) {
    if (!a.some((other) => JSON.stringify(item) === JSON.stringify(other))) {
      lines.push(`+ ${path}[] = ${render(item)}`);
    }
  }
  return lines;
}

function unionKeys(a: Record<string, unknown>, b: Record<string, unknown>): string[] {
  return [...new Set([...Object.keys(a), ...Object.keys(b)])].sort();
}

function render(value: unknown): string {
  const text = JSON.stringify(value) ?? 'undefined';
  return text.length > 72 ? `${text.slice(0, 69)}…` : text;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
