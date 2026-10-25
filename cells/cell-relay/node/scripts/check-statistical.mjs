import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { checkStatistical } from './statistical-report.mjs';
import { checkBypass, checkRegression } from './bypass-report.mjs';
const directory = resolve(process.argv[2] ?? 'target/wasm-statistical');
/** @type {string[]} */ const failures = [];
for (const [file, check] of /** @type {const} */ ([['report.json', checkStatistical], ['bypass.json', checkBypass]])) {
  try {
    const report = /** @type {unknown} */ (JSON.parse(readFileSync(resolve(directory, file), 'utf8')));
    check(report);
    if (process.env['AGINDOW_PERF_BASELINE'] !== undefined) checkRegression(report,
      JSON.parse(readFileSync(resolve(process.env['AGINDOW_PERF_BASELINE'], file), 'utf8')));
  } catch (error) { failures.push(`${file}: ${String(error)}`); }
}
if (failures.length > 0) throw Error(failures.join('\n'));
console.log('Release-WASM absolute gates passed. Regression baseline: ' + (process.env['AGINDOW_PERF_BASELINE'] ?? 'not yet qualified; explicit bootstrap only'));
