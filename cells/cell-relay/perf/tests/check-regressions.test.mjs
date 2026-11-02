import { test } from 'node:test';
import assert from 'node:assert/strict';
import { compare } from '../check-regressions.mjs';

const baseline = { a: 100, b: 200 };
test('allows exactly 10%, improvements and unchanged means', () => {
  assert.equal(compare(baseline, { a: 110, b: 180 }).passed, true);
});
test('fails any regression above 10%', () => {
  const report = compare(baseline, { a: 110.001, b: 200 });
  assert.equal(report.passed, false);
  assert.equal(report.rows[0]?.passed, false);
});
test('requires identical nonempty benchmark sets', () => {
  for (const candidate of [{ a: 100 }, { ...baseline, c: 1 }, {}]) {
    assert.throws(() => compare(baseline, candidate));
  }
  assert.throws(() => compare({}, {}));
});
test('rejects invalid or nonpositive estimates in either cohort', () => {
  for (const value of [NaN, Infinity, 0, -1]) {
    assert.throws(() => compare({ a: value }, { a: 1 }));
    assert.throws(() => compare({ a: 1 }, { a: value }));
  }
});

test('reads actual Criterion layout and rejects incomplete or malformed reports', async () => {
  const { mkdtempSync, mkdirSync, writeFileSync, rmSync } = await import('node:fs');
  const { tmpdir } = await import('node:os');
  const { join } = await import('node:path');
  const { readMeans } = await import('../check-regressions.mjs');
  const root = mkdtempSync(join(tmpdir(), 'native-perf-'));
  const path = join(root, 'suite', 'new');
  mkdirSync(path, { recursive: true });
  const mean = { point_estimate: 100, confidence_interval: { confidence_level: 0.95, lower_bound: 90, upper_bound: 110 } };
  try {
    writeFileSync(join(path, 'benchmark.json'), JSON.stringify({ full_id: 'suite/case' }));
    assert.throws(() => readMeans(root));
    writeFileSync(join(path, 'estimates.json'), JSON.stringify({ mean }));
    assert.deepEqual(readMeans(root), { 'suite/case': 100 });
    for (const interval of [
      { confidence_level: 0.9, lower_bound: 90, upper_bound: 110 },
      { confidence_level: 0.95, lower_bound: 101, upper_bound: 110 },
      { confidence_level: 0.95, lower_bound: 90, upper_bound: 99 },
    ]) {
      writeFileSync(join(path, 'estimates.json'), JSON.stringify({ mean: { ...mean, confidence_interval: interval } }));
      assert.throws(() => readMeans(root));
    }
    writeFileSync(join(path, 'estimates.json'), '{broken');
    assert.throws(() => readMeans(root));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
