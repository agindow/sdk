/**
 * Bounded, detached JSON admission shared by the semantic contract parsers.
 *
 * `ajv` proves the structural shape; this proves the value is plain JSON within
 * explicit resource bounds before it is detached from the caller and frozen.
 * Cycles, non-JSON objects, lone surrogates and oversized text are rejected
 * whole — never truncated into something that looks complete.
 */

export interface JsonBounds {
  /** Maximum serialized UTF-8 bytes. */
  readonly bytes: number;
  /** Maximum nesting depth. */
  readonly depth: number;
  /** Maximum visited nodes. */
  readonly nodes: number;
}

const encoder = new TextEncoder();

export function copyBounded(value: unknown, bounds: JsonBounds): unknown {
  const seen = new WeakSet<object>();
  let nodes = 0;
  let characters = 0;
  const text = (item: string): void => {
    characters += item.length;
    if (characters > bounds.bytes || /[\uD800-\uDFFF]/u.test(item)) throw new Error('invalid/oversized JSON text');
  };
  const walk = (item: unknown, depth: number): void => {
    if (++nodes > bounds.nodes || depth > bounds.depth) throw new Error('JSON resource limit');
    if (typeof item === 'string') {
      text(item);
      return;
    }
    if (item === null || typeof item === 'boolean') return;
    if (typeof item === 'number' && Number.isFinite(item)) return;
    if (typeof item !== 'object' || seen.has(item)) throw new Error('invalid JSON');
    const prototype: unknown = Object.getPrototypeOf(item);
    if (!Array.isArray(item) && prototype !== Object.prototype && prototype !== null) throw new Error('invalid JSON object');
    seen.add(item);
    for (const [key, child] of Object.entries(item)) {
      if (!Array.isArray(item)) text(key);
      walk(child, depth + 1);
    }
    seen.delete(item);
  };
  walk(value, 0);
  const serialized = JSON.stringify(value);
  if (serialized === undefined || encoder.encode(serialized).length > bounds.bytes) throw new Error('JSON byte limit');
  const copied: unknown = JSON.parse(serialized);
  return copied;
}

export function freezeValue<T>(value: T): T {
  if (typeof value === 'object' && value !== null && !Object.isFrozen(value)) {
    for (const child of Object.values(value)) freezeValue(child);
    Object.freeze(value);
  }
  return value;
}

/** Key-order-independent serialization for revision/history equality. */
export function canonicalJson(value: unknown): string {
  const sorted = (item: unknown): unknown => {
    if (Array.isArray(item)) return item.map(sorted);
    if (typeof item === 'object' && item !== null) {
      return Object.fromEntries(Object.entries(item).sort(([a], [b]) => a.localeCompare(b)).map(([key, child]) => [key, sorted(child)]));
    }
    return item;
  };
  return JSON.stringify(sorted(value));
}
