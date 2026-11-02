import { readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';

/** @param {unknown} value @returns {Record<string, unknown>} */
function record(value) {
  if (value === null || typeof value !== 'object' || Array.isArray(value)) throw new Error('expected object');
  return /** @type {Record<string, unknown>} */ (value);
}
/** @param {unknown} value @returns {number} */
function positive(value) {
  if (typeof value !== 'number' || !Number.isFinite(value) || value <= 0) throw new Error('invalid mean');
  return value;
}
/** Read only fresh Criterion estimates, keyed by benchmark identity.
 * @param {string} root @returns {Record<string, number>}
 */
export function readMeans(root) {
  /** @type {Record<string, number>} */
  const result = {};
  /** @param {string} dir */
  function visit(dir) {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const path = join(dir, entry.name);
      if (!entry.isDirectory() || entry.isSymbolicLink()) continue;
      if (entry.name !== 'new') { visit(path); continue; }
      const metadata = record(JSON.parse(readFileSync(join(path, 'benchmark.json'), 'utf8')));
      const id = metadata['full_id'];
      if (typeof id !== 'string' || id.length === 0 || Object.hasOwn(result, id)) throw new Error('invalid/duplicate benchmark id');
      const mean = record(record(JSON.parse(readFileSync(join(path, 'estimates.json'), 'utf8')))['mean']);
      const point = positive(mean['point_estimate']);
      const interval = record(mean['confidence_interval']);
      if (interval['confidence_level'] !== 0.95 || positive(interval['lower_bound']) > point || positive(interval['upper_bound']) < point) {
        throw new Error('invalid mean confidence interval');
      }
      result[id] = point;
    }
  }
  visit(root);
  return result;
}
/** @param {Record<string, number>} baseline @param {Record<string, number>} candidate */
export function compare(baseline, candidate) {
  const names = Object.keys(baseline).sort();
  if (names.length === 0 || JSON.stringify(names) !== JSON.stringify(Object.keys(candidate).sort())) {
    throw new Error('baseline/candidate benchmark sets differ or are empty');
  }
  const rows = names.map(name => {
    const before = positive(baseline[name]);
    const after = positive(candidate[name]);
    const ratio = after / before;
    return { name, baselineNs: before, candidateNs: after, changePercent: (ratio - 1) * 100, passed: ratio <= 1.10 };
  });
  return { passed: rows.every(row => row.passed), rows };
}
