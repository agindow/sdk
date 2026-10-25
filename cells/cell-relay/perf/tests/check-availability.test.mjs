import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';
import { checkAvailability } from '../check-availability.mjs';

const cases = ['not_ready', 'open_breaker', 'fresh_error', 'normal_inspection'];
/** @param {string} root @param {string} name @param {number} lower @param {number} upper */
function report(root, name, lower, upper) {
  const path = join(root, name, 'new');
  mkdirSync(path, { recursive: true });
  writeFileSync(join(path, 'estimates.json'), JSON.stringify({ mean: {
    point_estimate: (lower + upper) / 2,
    confidence_interval: { confidence_level: 0.95, lower_bound: lower, upper_bound: upper },
  } }));
}
/** @param {(root: string) => void} run */
function fixture(run) {
  const root = mkdtempSync(join(tmpdir(), 'agindow-ti42-'));
  try {
    for (const name of cases) report(root, name, name === 'normal_inspection' ? 20000 : 50,
      name === 'normal_inspection' ? 21000 : 60);
    run(root);
  } finally { rmSync(root, { recursive: true, force: true }); }
}

test('accepts all three bypass cases below the ceiling and normal comparison', () => {
  fixture(root => assert.equal(checkAvailability(root).length, 3));
});

test('rejects every bypass at the inclusive 10us boundary using the confidence upper bound', () => {
  for (const name of cases.slice(0, 3)) fixture(root => {
    report(root, name, 8000, 10000);
    assert.throws(() => checkAvailability(root), /availability-gate budget.*10000/);
  });
});

test('rejects bypass slower than normal even when below the absolute ceiling', () => {
  fixture(root => {
    report(root, 'normal_inspection', 60, 70);
    assert.throws(() => checkAvailability(root), /availability-gate.*normal inspection/);
  });
});

test('fails closed for missing, corrupt and non-finite reports', () => {
  for (const name of cases) fixture(root => {
    rmSync(join(root, name), { recursive: true });
    assert.throws(() => checkAvailability(root), /availability-gate.*report/);
  });
  for (const text of ['bad json', '{}', '{"mean":null}', '{"mean":{"point_estimate":1e999}}']) {
    fixture(root => {
      writeFileSync(join(root, 'fresh_error', 'new', 'estimates.json'), text);
      assert.throws(() => checkAvailability(root), /availability-gate.*report/);
    });
  }
});

test('CLI exits nonzero and diagnoses a deliberately lowered threshold', () => {
  fixture(root => {
    const run = spawnSync(process.execPath,
      [new URL('../check-availability.mjs', import.meta.url).pathname, root, '1'], { encoding: 'utf8' });
    assert.equal(run.status, 1);
    assert.match(run.stderr, /availability-gate budget/);
  });
});

test('rejects invalid confidence intervals and nonpositive budgets', () => {
  for (const interval of [
    { confidence_level: 0.9, lower_bound: 1, upper_bound: 3 },
    { confidence_level: 0.95, lower_bound: 3, upper_bound: 1 },
    { confidence_level: 0.95, lower_bound: -1, upper_bound: 3 },
    { confidence_level: 0.95, lower_bound: 1, upper_bound: null },
  ]) fixture(root => {
    writeFileSync(join(root, 'not_ready', 'new', 'estimates.json'), JSON.stringify({
      mean: { point_estimate: 2, confidence_interval: interval },
    }));
    assert.throws(() => checkAvailability(root), /availability-gate.*report/);
  });
  for (const budget of [0, -1, NaN, Infinity]) fixture(root => {
    assert.throws(() => checkAvailability(root, budget), /positive finite/);
  });
});
