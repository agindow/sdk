import { execFileSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import { WasmGuard } from '../guard.js';

let directory: string;
let module: WebAssembly.Module;

beforeAll(async () => {
  directory = mkdtempSync(join(tmpdir(), 'agindow-ti40-'));
  const binary = join(directory, 'panic.wasm');
  execFileSync('rustc', [
    new URL('./fixtures/panic.rs', import.meta.url).pathname,
    '--target', 'wasm32-unknown-unknown', '-C', 'panic=abort', '-o', binary,
  ]);
  module = await WebAssembly.compile(readFileSync(binary));
});

afterAll(() => { rmSync(directory, { recursive: true, force: true }); });

describe('WASM availability boundary', () => {
  it('contains a real Rust panic trap, retires the guest and keeps the host callable', async () => {
    const instance = await WebAssembly.instantiate(module);
    const main = instance.exports['main'];
    if (typeof main !== 'function') throw new Error('missing Rust test export');
    let inspections = 0;
    const guard = new WasmGuard(() => { inspections++; main(); });
    const original = { bytes: new Uint8Array([0, 255, 42]) };
    let calls = 0;
    const first = guard.run(() => { calls++; return original; });
    expect(first.status).toBe('bypassed');
    expect(first.outcome).toBe(original);
    const second = guard.run(() => { calls++; return original; });
    expect(second.status).toBe('bypassed');
    expect(second.outcome).toBe(original);
    expect(inspections).toBe(1);
    expect(calls).toBe(2);
    expect(new WasmGuard(() => {}).run(() => original)).toEqual({
      status: 'inspected', outcome: original,
    });
  });

  it('preserves the exact original error after an inspection exception', () => {
    const error = new Error('vendor failure');
    let calls = 0;
    const guard = new WasmGuard(() => { throw new Error('guest binding failure'); });
    let caught: unknown;
    try { guard.run(() => { calls++; throw error; }); }
    catch (failure: unknown) { caught = failure; }
    expect(caught).toBe(error);
    expect(calls).toBe(1);
  });

  it('does not retire a healthy guest when the original operation throws', () => {
    const error = { vendor: 'failure' };
    let inspections = 0;
    const guard = new WasmGuard(() => { inspections++; });
    try {
      guard.run(() => { throw error; });
      expect.unreachable('swallowed caller error');
    } catch (caught: unknown) {
      expect(caught).toBe(error);
    }
    expect(guard.run(() => 43)).toEqual({ status: 'inspected', outcome: 43 });
    expect(inspections).toBe(2);
  });
});
