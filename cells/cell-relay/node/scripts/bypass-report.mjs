import { validateEnvironment } from './performance-environment.mjs';
import { summarize } from './statistical-report.mjs';
/** @param {unknown} value @returns {Record<string, unknown>} */
const object = value => {
  if (value === null || typeof value !== 'object' || Array.isArray(value)) throw Error('invalid bypass report');
  return /** @type {Record<string, unknown>} */ (value);
};
/** @param {unknown} input @param {number} [limit] */
export function checkBypass(input, limit = 10) {
  const report = object(input);
  validateEnvironment(report['environment']);
  const expected = ['not-ready', 'budget', 'open-breaker', 'missing-snapshot', 'corrupt-snapshot', 'trap-retirement'];
  if (report['schema'] !== 'agindow.wasm-bypass/1' || report['samples'] !== 5000 || report['warmups'] !== 1000
    || !Array.isArray(report['cases']) || report['cases'].length !== expected.length
    || !Array.isArray(report['lifecycle']) || report['lifecycle'].length !== 20) throw Error('incomplete bypass matrix');
  for (const timing of report['lifecycle']) for (const field of ['loadUs', 'firstCallUs']) {
    if (!Number.isFinite(object(timing)[field]) || Number(object(timing)[field]) <= 0) throw Error('invalid lifecycle timing');
  }
  for (const kind of expected) {
    const entry = object(report['cases'].find(value => object(value)['kind'] === kind));
    if (entry['calls'] !== 11001 || entry['bypasses'] !== 6001 || (kind === 'trap-retirement' && entry['guestEntries'] !== 1)) throw Error('wrong bypass branch counts');
    if (kind === 'trap-retirement' && (entry['recoveryCalls'] !== 1 || !Number.isFinite(entry['recoverySetupUs'])
      || Number(entry['recoverySetupUs']) <= 0 || !Number.isFinite(entry['recoveryFirstUs']) || Number(entry['recoveryFirstUs']) <= 0)) throw Error('missing guest recovery evidence');
    const faults = object(entry['faults']);
    const expectedFault = kind === 'not-ready' ? 'NotReady' : kind === 'budget' ? 'ResourceExhausted' : kind === 'open-breaker' ? 'CircuitOpen' : undefined;
    if (expectedFault !== undefined && faults[expectedFault] !== 6001) throw Error('wrong guest availability branch');
    if ((kind === 'missing-snapshot' || kind === 'corrupt-snapshot') && entry['setupFailure'] !== true) throw Error('snapshot rejection unproven');
    for (const field of ['setupUs', 'firstUs']) if (!Number.isFinite(entry[field]) || Number(entry[field]) <= 0) throw Error('missing disposal/setup timing');
    const raw = object(entry['raw']); const summary = object(entry['summary']);
    for (const name of ['guarded', 'unguarded', 'added']) {
      const values = raw[name];
      if (!Array.isArray(values) || values.length !== 5000 || values.some(n => typeof n !== 'number' || !Number.isFinite(n)
        || (name !== 'added' && n < 0))) throw Error('invalid bypass samples');
      for (const [field, number] of Object.entries(summarize(values))) if (object(summary[name])[field] !== number) throw Error('invalid bypass quantiles');
    }
    for (let i = 0; i < 5000; i++) if (/** @type {number[]} */ (raw['added'])[i]
      !== Number(/** @type {number[]} */ (raw['guarded'])[i]) - Number(/** @type {number[]} */ (raw['unguarded'])[i])) throw Error('unpaired bypass samples');
    // Native TI-42 means remain a separate gate. This includes the Node boundary.
    if (Number(object(summary['guarded'])['mean']) >= limit) throw Error(`WASM bypass mean budget exceeded: ${kind}`);
  }
}
/** Matching runtime/hardware and matrix required before a baseline is comparable.
 * @param {unknown} current @param {unknown} previous */
export function checkRegression(current, previous) {
  const next = object(current); const prior = object(previous);
  if (next['schema'] !== prior['schema'] || JSON.stringify(next['environment']) !== JSON.stringify(prior['environment'])
    || !Array.isArray(next['cases']) || !Array.isArray(prior['cases']) || next['cases'].length !== prior['cases'].length) throw Error('incompatible regression baseline');
  for (const value of next['cases']) {
    const entry = object(value); const identifier = entry['id'] ?? entry['kind'];
    const before = object(prior['cases'].find(value => { const item = object(value); return (item['id'] ?? item['kind']) === identifier; }));
    for (const [field, factor] of /** @type {const} */ ([['p50', 1.15], ['p99', 1.25]])) {
      const observed = Number(object(object(entry['summary'])['guarded'])[field]);
      const baseline = Number(object(object(before['summary'])['guarded'])[field]);
      if (!Number.isFinite(observed) || !Number.isFinite(baseline) || baseline <= 0 || observed > baseline * factor) throw Error(`latency regression: ${String(identifier)}/${field}`);
    }
  }
}
