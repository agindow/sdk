import { performanceEnvironment } from './performance-environment.mjs';
import { performance } from 'node:perf_hooks';
import { mkdirSync, writeFileSync, readFileSync, mkdtempSync, rmSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createRequire } from 'node:module';
import { createAgindow } from '../dist/esm/index.js';
import { loadWasm, bindGuest } from '../dist/esm/wasm.js';
import { summarize } from './statistical-report.mjs';
const environment = performanceEnvironment();
const output = new URL('../../../../target/wasm-statistical/', import.meta.url);
mkdirSync(output, { recursive: true });
const selection = { tenant: 't', connection: 'c', vendor: 'v', operation: 'op', segment: 's',
  fields: [{ side: /** @type {const} */ ('request'), path: '/value', baseline: Array(128).fill('{"value":1}'), live: Array(128).fill('{"value":1}'), enumeration: true }] };
const value = { value: 1, padding: 'x'.repeat(838) };
/** @type {Record<string, unknown>[]} */ const cases = [];
/** @type {Record<string, unknown>[]} */ const lifecycle = [];
for (let i = 0; i < 20; i++) {
  const started = performance.now(); const engine = await loadWasm({ selection });
  const loaded = performance.now(); const call = engine.begin(value, 'observe', 'read', true, 'op'); call.finish(value); call.close();
  lifecycle.push({ loadUs: (loaded - started) * 1000, firstCallUs: (performance.now() - loaded) * 1000 });
}
const temporary = mkdtempSync(join(tmpdir(), 'agindow-bypass-'));
try {
  const trapFile = join(temporary, 'panic.wasm');
  execFileSync('rustc', [new URL('../../wasm/tests/fixtures/panic.rs', import.meta.url).pathname, '--target', 'wasm32-unknown-unknown', '-C', 'panic=abort', '-o', trapFile]);
  const module = await WebAssembly.compile(readFileSync(trapFile));
  for (const kind of ['not-ready', 'budget', 'open-breaker', 'missing-snapshot', 'corrupt-snapshot', 'trap-retirement']) {
    let calls = 0; let bypasses = 0; let guestEntries = 0;
    /** @type {import('../src/types.js').Engine | undefined} */ let engine;
    let setupFailure = false;
    let resetBudget = () => {};
    /** @type {Record<string, number>} */ const faults = {};
    const setup = performance.now();
    if (kind === 'missing-snapshot' || kind === 'corrupt-snapshot') {
      try { engine = await loadWasm({ selection: kind === 'missing-snapshot' ? { ...selection, fields: [{ side: 'request', path: '/value', live: Array.from({ length: 128 }, () => '{"value":1}'), enumeration: true, baseline: [] }] } : { ...selection, fields: [{ side: 'request', path: '/value', live: Array.from({ length: 128 }, () => '{"value":1}'), enumeration: true, baseline: ['invalid'] }] } }); }
      catch { setupFailure = true; }
      if (!setupFailure) throw Error(`${kind} unexpectedly loaded`);
    } else if (kind === 'trap-retirement') {
      const instance = await WebAssembly.instantiate(module); const panic = instance.exports['main'];
      if (typeof panic !== 'function') throw Error('missing real Rust panic fixture');
      engine = bindGuest({ active: () => false, begin() { guestEntries++; panic(); throw Error('unreachable'); } }, {});
    } else if (kind === 'budget') {
      const { Guest } = createRequire(import.meta.url)('../dist/guest/agw_node_guest.js');
      const resetBytes = new TextEncoder().encode('{"value":1}');
      const guest = new Guest(JSON.stringify(selection), performance.now());
      engine = bindGuest(guest, { selection, work: 0 });
      resetBudget = () => { const reset = guest.begin(resetBytes, false, 'observe', 'read', performance.now(), true, 200_000_000);
        reset.finish(resetBytes, false, performance.now()); };
    } else {
      engine = await loadWasm({ selection, ...(kind === 'budget' || kind === 'open-breaker' ? { work: 0 } : {}) });
      if (kind === 'not-ready') engine.setReady?.(false);
      if (kind === 'open-breaker') for (let i = 0; i < 3; i++) { const call = engine.begin(value, 'observe', 'read', true, 'op'); call.close(); }
    }
    if (engine !== undefined) {
      const selected = engine; engine = { ...selected, begin(...args) {
        const call = selected.begin(...args);
        const fault = call.request.fault;
        if (fault !== undefined && fault !== null) faults[fault] = (faults[fault] ?? 0) + 1;
        return call;
      } };
    }
    const setupUs = (performance.now() - setup) * 1000;
    const sdk = createAgindow({ spans: false, onEvent: event => { bypasses += Number(event.type === 'bypass'); } }, async () => {
      if (engine === undefined) throw Error('snapshot unavailable'); return engine;
    }); await sdk.ready;
    const original = () => { calls++; return value; };
    const run = sdk.guard(original, { tool: 'op', operation: 'read' });
    resetBudget(); const first = performance.now(); if (run() !== value) throw Error('lost first outcome'); const firstUs = (performance.now() - first) * 1000;
    for (let i = 0; i < 1000; i++) { resetBudget(); if (run() !== value) throw Error('lost warmup outcome'); }
    const raw = { guarded: /** @type {number[]} */ ([]), unguarded: /** @type {number[]} */ ([]), added: /** @type {number[]} */ ([]) };
    for (let i = 0; i < 5000; i++) {
      let guarded = 0; let unguarded = 0;
      for (const branch of i % 2 ? ['guarded', 'unguarded'] : ['unguarded', 'guarded']) {
        if (branch === 'guarded') resetBudget();
        const started = performance.now(); const result = branch === 'guarded' ? run() : original();
        const elapsed = (performance.now() - started) * 1000;
        if (result !== value) throw Error('lost original outcome');
        if (branch === 'guarded') guarded = elapsed; else unguarded = elapsed;
      }
      raw.guarded.push(guarded); raw.unguarded.push(unguarded); raw.added.push(guarded - unguarded);
    }
    if (calls !== 11001 || bypasses !== 6001 || (kind === 'trap-retirement' && guestEntries !== 1)) throw Error('wrong containment branch counts');
    if (kind === 'budget' && faults['ResourceExhausted'] !== 6001) throw Error('budget branch did not execute');
    let recoverySetupUs = 0; let recoveryFirstUs = 0; let recoveryCalls = 0;
    if (kind === 'trap-retirement') {
      const recover = performance.now(); const replacement = await loadWasm({ selection });
      recoverySetupUs = (performance.now() - recover) * 1000;
      const next = performance.now(); const call = replacement.begin(value, 'observe', 'read', true, 'op');
      if (call.request.bypassed || call.request.halt) throw Error('recovery request failed');
      const original = () => { recoveryCalls++; return value; };
      const result = original(); const verdict = call.finish(result); call.close();
      if (result !== value || verdict.bypassed || verdict.halt || guestEntries !== 1) throw Error('guest recovery failed');
      recoveryFirstUs = (performance.now() - next) * 1000;
    }
    const summary = Object.fromEntries(Object.entries(raw).map(([key, samples]) => [key, summarize(samples)]));
    cases.push({ kind, setupUs, firstUs, calls, bypasses, guestEntries, setupFailure, faults, recoverySetupUs, recoveryFirstUs, recoveryCalls, raw, summary });
  }
} finally { rmSync(temporary, { recursive: true, force: true }); }
const report = { schema: 'agindow.wasm-bypass/1', samples: 5000, warmups: 1000, lifecycle, cases,
  estimator: 'nearest-rank individual call quantiles; alternating paired differences',
  environment,
  note: 'Trap first-call retirement and recovery setup are separate from warmed retired-guest calls. Budget reset calls execute outside timing solely to close the breaker between independent fault samples. Missing/corrupt snapshots are rejected during setup; calls use unavailable SDK state.' };
writeFileSync(new URL('bypass.json', output), JSON.stringify(report) + '\n');
for (const entry of cases) console.log(JSON.stringify({ kind: entry['kind'], summary: entry['summary'], firstUs: entry['firstUs'] }));
