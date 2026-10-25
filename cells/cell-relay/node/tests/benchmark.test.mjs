import { test } from 'node:test';
import assert from 'node:assert/strict';
import { checkLoop } from '../scripts/check-loop.mjs';
const fixture = () => ({ schema: 'agindow.sdk-loop/1', samples: 5000, steps: 12000, bypasses: 0, loadUs: 1,
  raw: { guarded: Array(5000).fill(10), unguarded: Array(5000).fill(1), added: Array(5000).fill(9) },
  summary: { guarded: { p50: 10, p99: 10 }, unguarded: { p50: 1, p99: 1 }, added: { p50: 9, p99: 9 } } });
test('SDK performance checker fails on missing/invalid data, bypass runs, false quantiles and exceeded budgets', () => {
  checkLoop(fixture());
  assert.throws(() => checkLoop(undefined));
  /** @type {((r: ReturnType<typeof fixture>) => void)[]} */
  const mutations = [r => { r.raw = { guarded: [], unguarded: [], added: [] }; }, r => { r.raw.added[0] = NaN; },
    r => { r.raw.guarded.pop(); }, r => { r.bypasses = 1; }, r => { r.samples = 1; },
    r => { r.summary.added.p50 = 0; }];
  for (const mutate of mutations) {
    const report = fixture(); mutate(report); assert.throws(() => checkLoop(report));
  }
  assert.throws(() => checkLoop(fixture(), 9));
});
