// Functional probe only: never substitutes for the individual-call timing gate.
import assert from 'node:assert/strict';
import { loadWasm } from '../dist/esm/wasm.js';
import { matrix, fixture, expectations } from './statistical-fixtures.mjs';
for (const entry of matrix) {
  const f = fixture(entry); const expected = expectations(entry);
  const engine = await loadWasm({ selection: { tenant: 't', connection: 'c', vendor: 'v', operation: 'op', segment: 's', materiality: f.materiality,
    fields: (entry.phase === 'both' ? ['request', 'response'] : ['response']).map(side => ({
      side: /** @type {'request' | 'response'} */ (side), path: '/value', baseline: f.baseline, live: f.live, enumeration: true })) } });
  const call = engine.begin(f.input, entry.mode, entry.operation, true, 'op');
  const verdicts = [call.request]; let dispatched = 0;
  if (!call.request.halt) { dispatched++; verdicts.push(call.finish(f.input)); }
  call.close();
  assert.equal(dispatched, expected.requestHalt ? 0 : 1, entry.id);
  assert.equal(verdicts.some(v => v.halt), expected.halted, entry.id);
  assert.equal(verdicts.some(v => v.bypassed), false, entry.id);
  assert.equal(verdicts.reduce((sum, v) => sum + v.detectors, 0), expected.detectors / 6000, entry.id);
  if (expected.eligible !== undefined) assert.equal(verdicts.filter(v => v.eligibleDetectors?.includes(expected.eligible ?? '')).length, expected.phases, `${entry.id}: material detector`);
  if (expected.suppressed !== undefined) assert.equal(verdicts.filter(v => v.suppressedDetectors?.includes(expected.suppressed ?? '')).length, expected.phases, `${entry.id}: suppressed detector`);
}
console.log(`Validated ${matrix.length} actual release-WASM fixture branches (no timing qualification).`);
