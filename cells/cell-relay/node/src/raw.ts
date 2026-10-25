/** A complete, bounded raw inspection copy, never a replacement value. */
export class RawJson {
  constructor(readonly bytes: Uint8Array) {}
}
export const RAW_CAP = 1024 * 1024;
