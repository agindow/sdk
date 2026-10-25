import { RawJson, RAW_CAP } from './raw.js';

const encoder = new TextEncoder();
const decoder = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true });
const SURROGATES = /[\uD800-\uDFFF]/u;
// Backslashes are checked separately: a character class containing both a
// backslash and a quote defeats V8's fast class scan on long strings.
const NEEDS_QUOTE_ESCAPE = /["\u0000-\u001F]/u;
const MAX_BYTES = 1024 * 1024;

export interface Capture {
  json: string;
  truncated: boolean;
}
export interface CaptureBytes {
  bytes: Uint8Array;
  truncated: boolean;
}

/** Inspection copies only. Never invoke getters/toJSON or consume streams. */
export function capture(value: unknown): Capture {
  const projected = captureBytes(value);
  if (projected.truncated) return { json: '{}', truncated: true };
  return { json: decoder.decode(projected.bytes), truncated: false };
}

/** The same projection as {@link capture}, kept as UTF-8 for the guest. */
export function captureBytes(value: unknown): CaptureBytes {
  try {
    return { bytes: project(value), truncated: false };
  } catch {
    return { bytes: new Uint8Array([0x7b, 0x7d]), truncated: true };
  }
}

function project(value: unknown): Uint8Array {
  if (value instanceof RawJson) {
    if (value.bytes.byteLength > RAW_CAP) throw Error('inspection limit');
    // A fatal decode preserves the unavailable-projection rule for invalid
    // UTF-8; the original bytes stay untouched for byte-identical forwarding.
    decoder.decode(value.bytes);
    return value.bytes;
  }
  const bytes = encoder.encode(encode(value, 0));
  if (bytes.byteLength > MAX_BYTES) throw Error('inspection limit');
  return bytes;
}

function encode(item: unknown, depth: number): string {
  let nodes = 4096;
  let bytes = 1024 * 1024;
  const ancestors = new Set<object>();
  function visit(value: unknown, level: number): string {
    if (--nodes < 0 || level > 64) throw Error('inspection limit');
    if (value === null || typeof value === 'boolean') return String(value);
    if (typeof value === 'number' && Number.isFinite(value)) return String(value);
    if (typeof value === 'string') {
      // Rust strings cannot represent isolated UTF-16 surrogates. This is an
      // unavailable host projection, not malformed bytes received from a vendor.
      if (SURROGATES.test(value)) throw Error('unsupported Unicode');
      bytes -= value.length * 6;
      if (bytes < 0) throw Error('inspection limit');
      // Printable strings need no escaping; everything else keeps JSON.stringify's
      // exact escaping. This is the hot path for large ASCII payloads.
      return value.includes('\\') || NEEDS_QUOTE_ESCAPE.test(value) ? JSON.stringify(value) : `"${value}"`;
    }
    if (typeof value !== 'object' || value === null || ancestors.has(value)) throw Error('unsupported value');
    const array = Array.isArray(value);
    if (!array && Object.getPrototypeOf(value) !== Object.prototype && Object.getPrototypeOf(value) !== null) {
      throw Error('opaque object');
    }
    ancestors.add(value);
    const keys = Object.keys(value);
    if (keys.length > nodes) throw Error('inspection limit');
    const parts: string[] = [];
    for (const key of keys) {
      const descriptor = Object.getOwnPropertyDescriptor(value, key);
      if (descriptor === undefined || !('value' in descriptor)) throw Error('accessor');
      if (array && key !== String(parts.length)) throw Error('sparse/extended array');
      const content = visit(descriptor.value, level + 1);
      parts.push(array ? content : `${visit(key, level + 1)}:${content}`);
    }
    if (array && parts.length !== Reflect.get(value, 'length')) throw Error('sparse array');
    ancestors.delete(value);
    return array ? `[${parts.join(',')}]` : `{${parts.join(',')}}`;
  }
  return visit(item, depth);
}
