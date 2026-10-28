import { performance } from 'node:perf_hooks';
import { cpus, platform, release, arch } from 'node:os';
import { execFileSync } from 'node:child_process';
import { mkdirSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { resolve } from 'node:path';
import { createAgindow } from '../dist/esm/index.js';
import { checkLoop } from './check-loop.mjs';
const output = resolve(fileURLToPath(new URL('..', import.meta.url)), '../../../target/sdk-loop');
mkdirSync(output, { recursive: true });
/** @type {import('../src/types.js').Event[]} */
const events = [];
const cold = performance.now();
const sdk = createAgindow({ onEvent: e => events.push(e) }); await sdk.ready;
const loadUs = (performance.now() - cold) * 1000;
// A local agent loop plans a tool from its previous observation, invokes the
// ordinary promise-returning executor, and feeds the result back to the planner.
// Model and vendor network latency are excluded from both paired measurements.
const payload = { result: 'x'.repeat(849) }; // 862 bytes when JSON encoded
const tools = { lookup: () => Promise.resolve(payload), verify: () => Promise.resolve(payload) };
const guarded = sdk.wrap(tools);
let observation = { ...payload };
const plan = () => (observation.result.length + steps) % 2 === 0 ? 'lookup' : 'verify';
let steps = 0;
/** @param {typeof tools} target */
async function step(target) {
  const action = plan();
  observation = await target[action]();
  if (observation !== payload) throw Error('outcome identity lost');
  steps++;
}
/** @type {{guarded: number[], unguarded: number[], added: number[]}} */
const raw = { guarded: [], unguarded: [], added: [] };
for (let i = 0; i < 1000; i++) { await step(tools); await step(guarded); }
for (let i = 0; i < 5000; i++) {
  let guardedUs = NaN, unguardedUs = NaN;
  for (const kind of i % 2 === 0 ? ['guarded', 'unguarded'] : ['unguarded', 'guarded']) {
    const start = performance.now(); await step(kind === 'guarded' ? guarded : tools);
    const elapsed = (performance.now() - start) * 1000;
    if (kind === 'guarded') guardedUs = elapsed; else unguardedUs = elapsed;
  }
  raw.guarded.push(guardedUs); raw.unguarded.push(unguardedUs); raw.added.push(guardedUs - unguardedUs);
}
/** @param {number[]} values @param {number} p */
const quantile = (values, p) => [...values].sort((a, b) => a - b)[Math.ceil(values.length * p) - 1];
const summary = Object.fromEntries(Object.entries(raw).map(([key, values]) => {
  const mean = values.reduce((sum, value) => sum + value, 0) / values.length;
  return [key, { p50: quantile(values, .5), p99: quantile(values, .99), mean,
    stddev: Math.sqrt(values.reduce((sum, value) => sum + (value - mean) ** 2, 0) / values.length) }];
}));
const report = { schema: 'agindow.sdk-loop/1', fixture: 'local agent tool-selection/observation loop; default empty statistical plan through release WASM',
  estimator: 'p50 of alternating paired guarded-minus-unguarded individual loop steps; nearest-rank quantiles',
  samples: 5000, warmups: 1000, steps, payloadBytes: Buffer.byteLength(JSON.stringify(payload)),
  bypasses: events.filter(e => e.type === 'bypass').length, loadUs, summary, raw,
  environment: { node: process.version, v8: process.versions.v8, platform: platform(), os: release(), arch: arch(),
    cpu: cpus()[0]?.model, rust: execFileSync('rustc', ['--version'], { encoding: 'utf8' }).trim(),
    target: 'wasm32-unknown-unknown', profile: 'release', wasmBindgen: '0.2.129' },
  exclusions: ['module import before load timer', 'model inference', 'vendor/network', 'statistical paths (TI-53)'],
};
writeFileSync(resolve(output, 'report.json'), JSON.stringify(report, null, 2) + '\n');
checkLoop(report);
console.log(JSON.stringify({ addedUs: summary.added, guardedUs: summary.guarded,
  unguardedUs: summary.unguarded, loadUs, report: resolve(output, 'report.json') }, null, 2));
