import { execFileSync } from 'node:child_process';
import { readFileSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { expect, it } from 'vitest';
import { bindGuest } from '../src/wasm.js';
import { createAgindow } from '../src/index.js';

it('retires a real trapped Rust guest permanently and never retries the original operation', async () => {
  const directory = mkdtempSync(join(tmpdir(), 'agindow-sdk-trap-'));
  try {
    const binary = join(directory, 'panic.wasm');
    execFileSync('rustc', [new URL('../../wasm/tests/fixtures/panic.rs', import.meta.url).pathname,
      '--target', 'wasm32-unknown-unknown', '-C', 'panic=abort', '-o', binary]);
    const instance = await WebAssembly.instantiate(await WebAssembly.compile(readFileSync(binary)));
    const main = instance.exports['main'];
    if (typeof main !== 'function') throw Error('missing trap fixture');
    let entries = 0;
    const engine = bindGuest({ active: () => false, begin() { entries++; main(); throw Error('unreachable'); } }, {});
    const events: string[] = [];
    const sdk = createAgindow({ onEvent: e => events.push(e.type) }, async () => engine);
    await sdk.ready;
    const original = {};
    let calls = 0;
    const run = sdk.guard(() => { calls++; return original; });
    expect(run()).toBe(original);
    expect(run()).toBe(original);
    expect(entries).toBe(1);
    expect(calls).toBe(2);
    expect(events).toEqual(['error', 'bypass', 'error', 'bypass']);
  } finally { rmSync(directory, { recursive: true, force: true }); }
});
it('retires a guest whose explicit call disposal faults', async () => {
  let entries = 0;
  const bypass = JSON.stringify({ halt: false, bypassed: true, class: null,
    findings: 0, detectors: 0, malformed: false });
  const engine = bindGuest({ active: () => false, begin() {
    entries++;
    return { request: () => bypass, finish() { throw Error('drop trap'); }, free() { throw Error('drop trap'); } };
  } }, {});
  const events: string[] = [];
  const sdk = createAgindow({ onEvent: e => events.push(e.type) }, async () => engine); await sdk.ready;
  const value = {};
  const run = sdk.guard(() => value);
  expect(run()).toBe(value); expect(run()).toBe(value);
  expect(entries).toBe(1);
  expect(events).toEqual(['bypass', 'error', 'error', 'bypass']);
});
it('skips inspection copies after retiring the guest', () => {
  const engine = bindGuest({ active: () => true, begin() { throw Error('guest trap'); } }, {});
  expect(() => engine.begin({}, 'observe', 'read', true, 'op')).toThrow();
  let traversals = 0;
  const value = new Proxy({}, { ownKeys() { traversals++; return []; } });
  expect(() => engine.begin(value, 'observe', 'read', true, 'op')).toThrow();
  expect(traversals).toBe(0);
});
it('freezes binding scope and work settings after setup', () => {
  const selection = { tenant: 't', connection: 'c', vendor: 'v', operation: 'op', segment: 's', fields: [] };
  const options = { selection, work: 100 };
  const work: number[] = [];
  const json = JSON.stringify({ halt: false, bypassed: false, class: null,
    findings: 0, detectors: 0, malformed: false });
  const engine = bindGuest({ active: () => false, begin(_value, _truncated, _mode, _operation, _now, _closed, allowance) {
    work.push(allowance);
    return { request: () => json, finish: () => json, free() {} };
  } }, options);
  selection.operation = 'different'; options.work = 0;
  const call = engine.begin({}, 'observe', 'read', true, 'op'); call.close();
  expect(work).toEqual([100]);
});
it('advances the guest clock when closing an unfinished call', () => {
  const cleanup: { truncated: boolean; now: number }[] = [];
  const json = JSON.stringify({ halt: false, bypassed: false, class: null,
    findings: 0, detectors: 0, malformed: false });
  const engine = bindGuest({ active: () => false, begin() {
    return { request: () => json,
      finish(_body, truncated, now) { cleanup.push({ truncated, now }); return json; },
      free() { throw Error('untimed cleanup'); },
    };
  } }, {});
  const call = engine.begin({}, 'observe', 'read', true, 'op');
  call.close(); call.close();
  expect(cleanup).toHaveLength(1);
  expect(cleanup[0]?.truncated).toBe(true);
  expect(Number.isFinite(cleanup[0]?.now)).toBe(true);
});
