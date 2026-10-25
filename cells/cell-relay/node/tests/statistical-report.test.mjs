import { environment } from './performance-fixture.mjs';
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { expectations } from '../scripts/statistical-fixtures.mjs';
import { checkStatistical, matrix, summarize } from '../scripts/statistical-report.mjs';
const fixture = () => ({ environment: { ...environment }, schema: 'agindow.wasm-statistical/1', samples: 5000, warmups: 1000, payloadBytes: 862,
  coldUs: 1, wasmBytes: 100, cases: matrix.map(entry => {
    const expected = expectations(entry);
    return { ...entry, detectors: expected.detectors, detectorKinds: expected.kinds, bypasses: 0,
      calls: expected.requestHalt ? 0 : 6000, controls: 6000, halts: expected.halted ? 6000 : 0,
      eligible: expected.eligible === undefined ? {} : { [expected.eligible]: expected.phases * 6000 },
      suppressed: expected.suppressed === undefined ? {} : { [expected.suppressed]: expected.phases * 6000 },
      raw: { guarded: Array(5000).fill(10), unguarded: Array(5000).fill(1), added: Array(5000).fill(9) },
      summary: { guarded: summarize(Array(5000).fill(10)), unguarded: summarize(Array(5000).fill(1)), added: summarize(Array(5000).fill(9)) },
    };
  }) });
test('checks complete per-call evidence, pairing and every declared branch and threshold', () => {
  checkStatistical(fixture()); assert.throws(() => checkStatistical(undefined));
  /** @type {((report: ReturnType<typeof fixture>) => void)[]} */
  const mutations = [r => { r.cases.pop(); }, r => { r.samples = 1; }, r => { const c = r.cases[0]; if (c) c.controls = 0; }, r => { r.coldUs = NaN; },
    r => { const c = r.cases[0]; if (c) c.detectors = 0; }, r => { const c = r.cases[0]; if (c) c.halts = 1; },
    r => { const c = r.cases[0]; if (c) c.bypasses = 1; }, r => { const c = r.cases[0]; if (c) c.raw.guarded[0] = NaN; },
    r => { const c = r.cases[0]; if (c) c.raw.added[0] = 8; }, r => { const c = r.cases[0]; if (c) c.raw.guarded.pop(); },
    r => { const c = r.cases[0]; if (c) c.summary.guarded.p99 = 0; }];
  for (const mutation of mutations) { const report = fixture(); mutation(report); assert.throws(() => checkStatistical(report)); }
  assert.throws(() => checkStatistical(fixture(), { p50: 10, p99: 2000 }));
  assert.throws(() => checkStatistical(fixture(), { p50: 200, p99: 10 }));
});
test('rejects missing runtime/compiler metadata and mismatched fixture dimensions', () => {
  assert.throws(() => checkStatistical({ ...fixture(), environment: undefined }));
  const report = fixture(); const entry = report.cases[0]; if (!entry) throw Error('fixture'); entry.window = 999;
  assert.throws(() => checkStatistical(report));
});
