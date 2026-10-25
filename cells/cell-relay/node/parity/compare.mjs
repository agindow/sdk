import { execFileSync } from 'node:child_process';
/** @param {unknown} value @returns {Record<string, unknown>} */
function record(value) {
  if (value === null || typeof value !== 'object' || Array.isArray(value)) throw Error('invalid parity record');
  return /** @type {Record<string, unknown>} */ (value);
}
/** @param {unknown} value @returns {string} */
function canonical(value) {
  if (Array.isArray(value)) return `[${value.map(canonical).join(',')}]`;
  if (typeof value === 'object' && value !== null) {
    const object = record(value);
    return `{${Object.keys(object).sort().map(key => `${JSON.stringify(key)}:${canonical(object[key])}`).join(',')}}`;
  }
  return JSON.stringify(value) ?? 'null';
}
/** Refuse missing, duplicate, extra and divergent rows.
 * @param {unknown} definition @param {unknown} typescript @param {unknown} python */
export function compare(definition, typescript, python) {
  const cases = record(definition)['cases'];
  if (!Array.isArray(cases) || cases.length === 0 || !Array.isArray(typescript) || !Array.isArray(python)) throw Error('missing parity corpus');
  const expected = cases.map(item => record(item)['id']);
  if (new Set(expected).size !== expected.length || expected.some(id => typeof id !== 'string')) throw Error('invalid fixture IDs');
  for (const result of [typescript, python]) {
    if (result.length !== expected.length) throw Error('missing parity rows');
    const ids = result.map(row => record(row)['id']);
    if (new Set(ids).size !== ids.length || ids.some(id => !expected.includes(id))) throw Error('invalid result IDs');
  }
  for (const id of expected) {
    const left = typescript.find(row => record(row)['id'] === id);
    const right = python.find(row => record(row)['id'] === id);
    if (canonical(left) !== canonical(right)) throw Error(`cross-language divergence: ${String(id)}`);
  }
}
/** @param {unknown} definition @param {readonly string[]} ts @param {readonly string[]} py */
export function runParity(definition, ts, py) {
  const run = (/** @type {readonly string[]} */ command) => {
    const executable = command[0]; if (executable === undefined) throw Error('missing SDK parity command');
    return JSON.parse(execFileSync(executable, command.slice(1), {
      input: JSON.stringify(definition), encoding: 'utf8', timeout: 30_000, maxBuffer: 1024 * 1024,
    }));
  };
  compare(definition, run(ts), run(py));
}
