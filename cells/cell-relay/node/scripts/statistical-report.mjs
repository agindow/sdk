import { validateEnvironment } from './performance-environment.mjs';
/** @param {unknown} value @returns {Record<string, unknown>} */
const object = value => {
  if (value === null || typeof value !== 'object' || Array.isArray(value)) throw Error('invalid report object');
  return /** @type {Record<string, unknown>} */ (value);
};
export { matrix } from './statistical-fixtures.mjs';
import { matrix, expectations } from './statistical-fixtures.mjs';
/** @param {number[]} values */
export function summarize(values) {
  const sorted = [...values].sort((a, b) => a - b);
  const mean = values.reduce((a, b) => a + b, 0) / values.length;
  return { p50: sorted[Math.ceil(values.length * .5) - 1], p99: sorted[Math.ceil(values.length * .99) - 1], mean,
    stddev: Math.sqrt(values.reduce((sum, n) => sum + (n - mean) ** 2, 0) / values.length) };
}
/** @param {unknown} input @param {{p50: number, p99: number}} [limits] */
export function checkStatistical(input, limits = { p50: 200, p99: 2000 }) {
  const report = object(input);
  validateEnvironment(report['environment']);
  if (report['schema'] !== 'agindow.wasm-statistical/1' || report['samples'] !== 5000 || report['warmups'] !== 1000
    || report['payloadBytes'] !== 862 || !Array.isArray(report['cases']) || report['cases'].length !== matrix.length) throw Error('incomplete statistical matrix');
  if (!Number.isFinite(report['coldUs']) || Number(report['coldUs']) <= 0 || !Number.isSafeInteger(report['wasmBytes'])
    || Number(report['wasmBytes']) <= 0 || Number(report['wasmBytes']) >= 5_000_000) throw Error('missing lifecycle or size evidence');
  const ids = new Set();
  for (const expected of matrix) {
    const entry = object(report['cases'].find(value => object(value)['id'] === expected.id));
    if (ids.has(entry['id'])) throw Error('duplicate matrix case'); ids.add(entry['id']);
    for (const [key, value] of Object.entries(expected)) if (entry[key] !== value) throw Error('matrix metadata mismatch');
    const expectedBranch = expectations(expected);
    if (entry['detectors'] !== expectedBranch.detectors || entry['bypasses'] !== 0 || entry['controls'] !== 6000 || entry['calls'] !== (expectedBranch.requestHalt ? 0 : 6000)
      || entry['halts'] !== (expectedBranch.halted ? 6000 : 0) || entry['payloadBytes'] !== expected.payloadBytes) throw Error('wrong detector/policy branch counts');
    const kinds = entry['detectorKinds'];
    if (!Array.isArray(kinds) || JSON.stringify([...kinds].sort()) !== JSON.stringify(expectedBranch.kinds)) throw Error('wrong detector kinds');
    const eligible = object(entry['eligible']); const suppressed = object(entry['suppressed']);
    if (expectedBranch.eligible !== undefined && eligible[expectedBranch.eligible] !== expectedBranch.phases * 6000) throw Error('material detector did not execute');
    if (expectedBranch.suppressed !== undefined && suppressed[expectedBranch.suppressed] !== expectedBranch.phases * 6000) throw Error('suppressed detector did not execute');
    const raw = object(entry['raw']); const stats = object(entry['summary']);
    for (const key of ['guarded', 'unguarded', 'added']) {
      const values = raw[key];
      if (!Array.isArray(values) || values.length !== 5000 || values.some(n => typeof n !== 'number' || !Number.isFinite(n)
        || (key !== 'added' && n < 0))) throw Error('invalid raw sample');
      const actual = summarize(/** @type {number[]} */ (values)); const published = object(stats[key]);
      for (const [field, value] of Object.entries(actual)) if (published[field] !== value) throw Error('incorrect statistical summary');
    }
    for (let i = 0; i < 5000; i++) {
      const guarded = /** @type {number[]} */ (raw['guarded']); const unguarded = /** @type {number[]} */ (raw['unguarded']);
      if (/** @type {number[]} */ (raw['added'])[i] !== Number(guarded[i]) - Number(unguarded[i])) throw Error('unpaired samples');
    }
    const timing = object(stats['guarded']);
    if (Number(timing['p50']) >= limits.p50 || Number(timing['p99']) >= limits.p99) throw Error(`WASM statistical budget exceeded: ${expected.id}`);
  }
}
