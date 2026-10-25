import { environment } from './performance-fixture.mjs';
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { checkBypass, checkRegression } from '../scripts/bypass-report.mjs';
import { summarize } from '../scripts/statistical-report.mjs';
const fixture = () => ({ schema: 'agindow.wasm-bypass/1', samples: 5000, warmups: 1000, environment: { ...environment },
  lifecycle: Array.from({ length: 20 }, () => ({ loadUs: 1, firstCallUs: 1 })),
  cases: ['not-ready','budget','open-breaker','missing-snapshot','corrupt-snapshot','trap-retirement'].map(kind => {
    const raw = { guarded: Array.from({ length: 5000 }, () => 2), unguarded: Array.from({ length: 5000 }, () => 1), added: Array.from({ length: 5000 }, () => 1) };
    return { kind, calls: 11001, bypasses: 6001, guestEntries: kind === 'trap-retirement' ? 1 : 0, setupUs: 1, firstUs: 1, recoverySetupUs: 1, recoveryFirstUs: 1, recoveryCalls: 1, setupFailure: kind.includes('snapshot'),
      faults: { [kind === 'not-ready' ? 'NotReady' : kind === 'budget' ? 'ResourceExhausted' : 'CircuitOpen']: 6001 }, raw,
      summary: Object.fromEntries(Object.entries(raw).map(([key, values]) => [key, summarize(values)])) };
  }) });
test('validates individual bypass samples and rejects missing branches, no-op counts and threshold mutations', () => {
  checkBypass(fixture());
  assert.throws(() => checkBypass(fixture(), 1));
  /** @type {((report: ReturnType<typeof fixture>) => void)[]} */
  const mutations = [report => report.cases.pop(), report => { const entry = report.cases.find(c => c.kind === 'trap-retirement'); if (entry) entry.recoveryCalls = 0; }, report => { const entry = report.cases[0]; if (entry) entry.bypasses = 0; },
    report => { const entry = report.cases[0]; if (entry) entry.raw.guarded[0] = NaN; }];
  for (const mutate of mutations) {
    const report = fixture(); mutate(report); assert.throws(() => checkBypass(report));
  }
});
test('enforces predeclared p50/p99 regression factors and compatible environments', () => {
  const before = fixture(); const after = fixture(); checkRegression(after, before);
  const entry = after.cases[0]; if (!entry) throw Error('fixture'); entry.summary['guarded'] = { p50: 3, p99: 3, mean: 3, stddev: 0 };
  assert.throws(() => checkRegression(after, before));
  before.environment.node = 'different'; assert.throws(() => checkRegression(fixture(), before));
});
