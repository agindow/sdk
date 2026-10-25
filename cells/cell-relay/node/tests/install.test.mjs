import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { checkSize, packageReport } from '../scripts/size.mjs';
test('the actual npm tarball installs offline, resolves both formats and includes the release WASM', () => {
  const report = packageReport();
  const bytes = checkSize(report);
  assert.ok(bytes < 5_000_000);
  const directory = mkdtempSync(join(tmpdir(), 'agindow-install-'));
  const cwd = fileURLToPath(new URL('..', import.meta.url));
  try {
    const packed = /** @type {{ filename: string }[]} */ (JSON.parse(execFileSync('npm', ['pack', '--json', '--ignore-scripts', '--pack-destination', directory,
      '--cache', join(directory, 'cache')], { cwd, encoding: 'utf8' })));
    const first = packed[0];
    if (first === undefined) throw Error('empty pack report');
    execFileSync('npm', ['install', '--prefix', directory, '--offline', '--ignore-scripts', '--no-audit', '--no-fund',
      '--cache', join(directory, 'cache'), join(directory, first.filename)], { encoding: 'utf8' });
    /** @type {[string, string][]} */
    const formats = [
      ['module', "import { agindow } from 'agindow';"],
      ['commonjs', "const {agindow} = require('agindow');"],
    ];
    for (const [format, code] of formats) {
      const test = `${code}\n(async()=>{let bypass=0;await agindow.ready;const fn=agindow.guard(()=>42,{onEvent:()=>bypass++});if(fn()!==42||bypass)process.exit(1)})()`;
      execFileSync('node', ['--input-type', format, '-e', test], { cwd: directory, encoding: 'utf8' });
    }
    console.log(`TI-30 unpacked npm package: ${bytes} bytes`);
  } finally { rmSync(directory, { recursive: true, force: true }); }
});
test('size validator rejects missing, malformed, incomplete and oversized packages', () => {
  for (const report of [undefined, [], [{}], [{ unpackedSize: NaN }], [{ unpackedSize: 5_000_000 }],
    [{ unpackedSize: 1, files: [] }]]) assert.throws(() => checkSize(report));
  assert.throws(() => checkSize(packageReport(), 1));
});
