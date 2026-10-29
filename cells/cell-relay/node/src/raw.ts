/** A complete, bounded raw inspection copy, never a replacement value. */
export class RawJson {
  constructor(readonly bytes: Uint8Array) {}
}
export const RAW_CAP = 1024 * 1024;

/** Restore an ordinary raw callable's representation after verified span repair.
 * HTTP adapters own their replacement hooks and keep RawJson internal.
 */
export function restoreRaw(original: unknown, replacement: unknown): string | Uint8Array {
  const bytes = replacement instanceof RawJson ? replacement.bytes : replacement;
  if (!(bytes instanceof Uint8Array) || bytes.byteLength > RAW_CAP) throw Error('raw repair unavailable');
  if (typeof original === 'string') return new TextDecoder('utf-8', { fatal: true, ignoreBOM: true }).decode(bytes);
  if (Buffer.isBuffer(original)) return Buffer.from(bytes);
  if (original instanceof Uint8Array) return new Uint8Array(bytes);
  throw Error('original raw representation unavailable');
}
