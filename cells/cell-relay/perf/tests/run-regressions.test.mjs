import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, copyFileSync, writeFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';

test('a failed run cannot reuse an earlier passing report', () => {
  const root = mkdtempSync(join(tmpdir(), 'native-regression-run-'));
  const perf = join(root, 'cells/cell-relay/perf');
  const output = join(root, 'target/native-regressions');
  mkdirSync(perf, { recursive: true });
  mkdirSync(output, { recursive: true });
  try {
    for (const file of ['run-regressions.mjs', 'check-regressions.mjs', 'baseline.v1.json']) {
      copyFileSync(new URL(`../${file}`, import.meta.url), join(perf, file));
    }
    writeFileSync(join(output, 'report.json'), JSON.stringify({ passed: true }));
    // No Git repository: baseline export must fail before benchmarks can run.
    const result = spawnSync(process.execPath, [join(perf, 'run-regressions.mjs')], {
      cwd: root, encoding: 'utf8', env: { ...process.env, GIT_CEILING_DIRECTORIES: root },
    });
    assert.notEqual(result.status, 0);
    assert.equal(existsSync(join(output, 'report.json')), false);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
