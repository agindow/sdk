import { expect, it } from 'vitest';
import { capture, captureBytes } from '../src/inspection.js';
import { RawJson } from '../src/raw.js';
it('never invokes accessors or toJSON and does not consume opaque responses', () => {
  let reads = 0;
  const value = { get secret() { reads++; throw Error('getter'); } };
  expect(capture(value).truncated).toBe(true);
  expect(reads).toBe(0);
  expect(capture({ toJSON() { reads++; } }).truncated).toBe(true);
  expect(reads).toBe(0);
  expect(capture(new Response('body')).truncated).toBe(true);
});
it('refuses cycles, excessive depth, large strings and sparse arrays', () => {
  const value: { self?: unknown } = {}; value.self = value;
  expect(capture(value).truncated).toBe(true);
  let deep: unknown = {};
  for (let i = 0; i < 130; i++) deep = { child: deep };
  expect(capture(deep).truncated).toBe(true);
  expect(capture('a'.repeat(1024 * 1024)).truncated).toBe(true);
  expect(capture(new Array(2)).truncated).toBe(true);
});
it('projects supported scalars and dense arrays without changing the input', () => {
  const value = { text: '😀\n', number: 42, list: [true, null, 'ok'] };
  const result = capture(value);
  expect(result.truncated).toBe(false);
  expect(JSON.parse(result.json)).toEqual(value);
});
it('includes JSON delimiters and escaping in the final byte cap', () => {
  const value = Array.from({ length: 2048 }, () => '\u0000'.repeat(85));
  expect(Buffer.byteLength(JSON.stringify(value))).toBeGreaterThan(1024 * 1024);
  expect(capture(value).truncated).toBe(true);
});
it('skips unpaired UTF-16 surrogates rather than inventing malformed vendor drift', () => {
  expect(capture({ value: '\ud800' }).truncated).toBe(true);
  expect(capture({ '\udfff': 'value' }).truncated).toBe(true);
  expect(capture({ value: '😀' }).truncated).toBe(false);
});
it('projects the same JSON bytes the guest parses, including escaped strings', () => {
  const value = { text: '😀\n', quote: 'a"b', slash: 'a\\b', control: '\u0000\u001f',
    number: 42, list: [true, null, 'ok'] };
  const projected = captureBytes(value);
  expect(projected.truncated).toBe(false);
  expect(new TextDecoder().decode(projected.bytes)).toBe(capture(value).json);
  expect(JSON.parse(new TextDecoder().decode(projected.bytes))).toEqual(value);
});
it('keeps the fast quoting path byte-identical for long printable strings', () => {
  const padding = 'x'.repeat(65536);
  const value = { value: 1, padding };
  const projected = captureBytes(value);
  expect(projected.truncated).toBe(false);
  expect(new TextDecoder().decode(projected.bytes)).toBe(JSON.stringify(value));
  expect(JSON.parse(new TextDecoder().decode(projected.bytes))).toEqual(value);
});
it('passes complete raw UTF-8 inspection bytes through without re-encoding', () => {
  const bytes = new TextEncoder().encode('{"value":"é😀"}');
  const projected = captureBytes(new RawJson(bytes));
  expect(projected.truncated).toBe(false);
  expect(projected.bytes).toBe(bytes);
  expect(new TextDecoder().decode(projected.bytes)).toBe(capture(new RawJson(bytes)).json);
});
it('treats invalid raw UTF-8 as an unavailable inspection projection', () => {
  expect(captureBytes(new RawJson(Uint8Array.from([0x7b, 0xff, 0x7d]))).truncated).toBe(true);
  expect(capture(new RawJson(Uint8Array.from([0x7b, 0xff, 0x7d]))).truncated).toBe(true);
});
it('truncates byte projections exactly when the string projection truncates', () => {
  const cyclic: { self?: unknown } = {}; cyclic.self = cyclic;
  for (const value of [cyclic, { value: '\ud800' }, new Array(2), 'a'.repeat(1024 * 1024)]) {
    expect(captureBytes(value).truncated).toBe(capture(value).truncated);
  }
});
