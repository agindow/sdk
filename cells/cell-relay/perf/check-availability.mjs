import { readFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

const BYPASS = ['not_ready', 'open_breaker', 'fresh_error'];
/** @typedef {{ lower: number, upper: number }} Estimate */
/** @param {unknown} value @returns {Record<string, unknown>} */
function record(value) {
  if (value === null || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error('expected object');
  }
  return /** @type {Record<string, unknown>} */ (value);
}
/** @param {unknown} value @returns {number} */
function positive(value) {
  if (typeof value !== 'number' || !Number.isFinite(value) || value <= 0) {
    throw new Error('expected positive finite nanoseconds');
  }
  return value;
}
/** @param {string} root @param {string} name @returns {Estimate} */
function estimate(root, name) {
  try {
    const raw = /** @type {unknown} */ (JSON.parse(readFileSync(join(root, name, 'new', 'estimates.json'), 'utf8')));
    const mean = record(record(raw)['mean']);
    const interval = record(mean['confidence_interval']);
    const point = positive(mean['point_estimate']);
    const lower = positive(interval['lower_bound']);
    const upper = positive(interval['upper_bound']);
    if (interval['confidence_level'] !== 0.95 || lower > point || point > upper) {
      throw new Error('invalid 95% mean confidence interval');
    }
    return { lower, upper };
  } catch (cause) {
    throw new Error(`availability-gate report ${name} is missing or invalid`, { cause });
  }
}

/** Fail closed on incomplete reports and use a conservative confidence bound.
 * @param {string} root @param {number} [budgetNs] @returns {string[]}
 */
export function checkAvailability(root, budgetNs = 10000) {
  positive(budgetNs);
  const normal = estimate(root, 'normal_inspection');
  return BYPASS.map(name => {
    const measured = estimate(root, name);
    if (measured.upper >= budgetNs) {
      throw new Error(`availability-gate budget: ${name} mean upper bound ${measured.upper.toFixed(2)} ns >= ${budgetNs} ns`);
    }
    if (measured.upper >= normal.lower) {
      throw new Error(`availability-gate ${name}: ${measured.upper.toFixed(2)} ns is not faster than normal inspection (${normal.lower.toFixed(2)} ns)`);
    }
    return `${name}: mean upper bound ${measured.upper.toFixed(2)} ns < ${budgetNs} ns; normal lower bound ${normal.lower.toFixed(2)} ns`;
  });
}

if (process.argv[1] !== undefined && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  try {
    const root = process.argv[2];
    if (root === undefined) throw new Error('availability-gate: expected Criterion availability report directory');
    const budget = process.argv[3] === undefined ? 10000 : Number(process.argv[3]);
    for (const line of checkAvailability(root, budget)) console.log(line);
  } catch (error) {
    console.error(error);
    process.exitCode = 1;
  }
}
