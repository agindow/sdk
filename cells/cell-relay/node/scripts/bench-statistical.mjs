import { performanceEnvironment } from './performance-environment.mjs';
import { performance } from 'node:perf_hooks';
import { mkdirSync, writeFileSync, statSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { resolve } from 'node:path';
import { loadWasm } from '../dist/esm/wasm.js';
import { fixture } from './statistical-fixtures.mjs';
import { matrix, summarize, checkStatistical } from './statistical-report.mjs';
const environment = performanceEnvironment();
const output = resolve(fileURLToPath(new URL('..', import.meta.url)), '../../../target/wasm-statistical');
mkdirSync(output, { recursive: true });
const payload = { value: 1, padding: 'x'.repeat(838) };
if (Buffer.byteLength(JSON.stringify(payload)) !== 862) throw Error('fixture size changed');
const coldStart = performance.now();
await loadWasm({}); const coldUs = (performance.now() - coldStart) * 1000;
/** @type {Record<string, unknown>[]} */
const cases = [];
for (const entry of matrix) {
  const mode = /** @type {import('../src/types.js').Mode} */ (entry.mode);
  const operation = /** @type {import('../src/types.js').Operation} */ (entry.operation);
  const { input, baseline, live, materiality } = fixture(entry);
  const engine = await loadWasm({ selection: { tenant: 't', connection: 'c', vendor: 'v', operation: 'op', segment: 's', materiality,
    fields: (entry.phase === 'both' ? ['request', 'response'] : ['response']).map(side => ({
      side: /** @type {'request' | 'response'} */ (side), path: '/value', baseline, live, enumeration: true })) } });
  let detectors = 0; let bypasses = 0; let halts = 0; let calls = 0; let controls = 0; let guardedEntry = false;
  const detectorKinds = new Set();
  /** @type {Record<string, number>} */ const eligible = {};
  /** @type {Record<string, number>} */ const suppressed = {};
  /** @param {import('../src/types.js').Verdict} verdict */
  const count = verdict => {
    detectors += verdict.detectors;
    for (const kind of verdict.detectorKinds ?? []) detectorKinds.add(kind);
    for (const kind of verdict.eligibleDetectors ?? []) eligible[kind] = (eligible[kind] ?? 0) + 1;
    for (const kind of verdict.suppressedDetectors ?? []) suppressed[kind] = (suppressed[kind] ?? 0) + 1;
  };
  const original = () => { if (guardedEntry) calls++; else controls++; return input; };
  const guarded = () => {
    guardedEntry = true;
    try {
    const call = engine.begin(input, mode, operation, true, 'op');
    let bypassed = call.request.bypassed;
    count(call.request);
    if (call.request.halt) { halts++; call.close(); }
    else {
      const outcome = original();
      const response = call.finish(outcome);
      if (outcome !== input) throw Error('identity lost');
      count(response); bypassed ||= response.bypassed;
      if (response.halt) halts++;
      call.close();
    }
    bypasses += Number(bypassed);
    } finally { guardedEntry = false; }
  };
  const raw = { guarded: /** @type {number[]} */ ([]), unguarded: /** @type {number[]} */ ([]), added: /** @type {number[]} */ ([]) };
  for (let i = 0; i < 1000; i++) { original(); guarded(); }
  for (let i = 0; i < 5000; i++) {
    let guardUs = NaN; let controlUs = NaN;
    for (const kind of i % 2 === 0 ? ['guarded', 'unguarded'] : ['unguarded', 'guarded']) {
      const start = performance.now();
      if (kind === 'guarded') guarded(); else original();
      const us = (performance.now() - start) * 1000;
      if (kind === 'guarded') guardUs = us; else controlUs = us;
    }
    raw.guarded.push(guardUs); raw.unguarded.push(controlUs); raw.added.push(guardUs - controlUs);
  }
  const summary = Object.fromEntries(Object.entries(raw).map(([key, values]) => [key, summarize(values)]));
  cases.push({ ...entry, detectors, detectorKinds: [...detectorKinds], eligible, suppressed, bypasses, halts, calls, controls, raw, summary });
  console.log(JSON.stringify({ id: entry.id, detectors, eligible, suppressed, halts, calls, guarded: summary['guarded'] }));
}
const report = { schema: 'agindow.wasm-statistical/1', samples: 5000, warmups: 1000, payloadBytes: 862,
  coldUs, wasmBytes: statSync(new URL('../dist/guest/agw_node_guest_bg.wasm', import.meta.url)).size, cases,
  estimator: 'nearest-rank individual-call quantiles; alternating paired differences',
  environment,
  exclusions: ['network', 'vendor', 'model', 'setup-only snapshot publication', 'cold compilation/instantiation (reported separately)'],
};
writeFileSync(resolve(output, 'report.json'), JSON.stringify(report) + '\n');
checkStatistical(report);
