import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
/** @param {unknown} value @returns {Record<string, unknown>} */
function record(value) {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) throw Error('invalid package');
  return /** @type {Record<string, unknown>} */ (value);
}
/** @param {unknown} report @param {number} [limit] */
export function checkSize(report, limit = 5_000_000) {
  if (!Array.isArray(report)) throw Error('missing package report');
  const pkg = record(report[0]);
  const size = pkg['unpackedSize'];
  if (typeof size !== 'number' || !Number.isSafeInteger(size) || size <= 0 || size >= limit) {
    throw Error(`installed package must be below ${limit} bytes`);
  }
  const files = pkg['files'];
  if (!Array.isArray(files)) throw Error('missing package files');
  const paths = files.map(file => record(file)['path']);
  if (!paths.some(path => typeof path === 'string' && path.endsWith('.wasm'))
    || !paths.includes('LICENSE') || !paths.some(path => typeof path === 'string' && path.endsWith('.d.ts'))
    || !paths.includes('dist/cjs/index.js') || !paths.includes('dist/esm/index.js')) throw Error('incomplete package');
  return size;
}
/** @returns {unknown} */
export function packageReport() {
  return JSON.parse(execFileSync('npm', ['pack', '--dry-run', '--json', '--ignore-scripts', '--cache', '/tmp/agindow-npm-cache'],
    { cwd: fileURLToPath(new URL('..', import.meta.url)), encoding: 'utf8' }));
}
