/** @param {unknown} value @returns {Record<string, unknown>} */
function record(value) {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) throw Error('invalid report');
  return /** @type {Record<string, unknown>} */ (value);
}
/** @param {unknown} input @param {number} [limit] */
export function checkLoop(input, limit = 100) {
  const report = record(input);
  if (report['schema'] !== 'agindow.sdk-loop/1' || typeof report['samples'] !== 'number' || report['samples'] < 5000
    || report['steps'] !== 12000 || report['bypasses'] !== 0 || typeof report['loadUs'] !== 'number'
    || !Number.isFinite(report['loadUs'])) throw Error('incomplete SDK loop');
  const raw = record(report['raw']);
  const summary = record(report['summary']);
  for (const key of ['guarded', 'unguarded', 'added']) {
    const values = raw[key];
    if (!Array.isArray(values) || values.length !== report['samples']
      || values.some(v => typeof v !== 'number' || !Number.isFinite(v))) throw Error('missing or invalid samples');
    const sorted = /** @type {number[]} */ ([...values]).sort((a, b) => a - b);
    const stats = record(summary[key]);
    for (const name of ['p50', 'p99']) {
      const quantile = name === 'p50' ? .5 : .99;
      if (stats[name] !== sorted[Math.ceil(values.length * quantile) - 1]) throw Error('incorrect quantile');
    }
  }
  const median = record(summary['added'])['p50'];
  if (typeof median !== 'number' || median >= limit) throw Error(`SDK added p50 must be below ${limit} µs`);
}
