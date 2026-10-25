import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { compare } from '../parity/compare.mjs';
const definition = JSON.parse(readFileSync(new URL('../parity/cases.json', import.meta.url), 'utf8'));
/** @type {Record<string, unknown>[]} */
const ts = JSON.parse(execFileSync('node', [new URL('../parity/typescript.mjs', import.meta.url).pathname], { input: JSON.stringify(definition), encoding: 'utf8' }));
test('generated TypeScript parity rows exercise the public wrapper and compare exhaustively', () => {
  assert.equal(ts.length, definition.cases.length);
  assert.doesNotThrow(() => compare(definition, ts, [...ts].reverse()));
  for (const mutation of [[], ts.slice(1), [...ts, ts[0]], ts.map((row, index) => index === 0 ? { ...row, calls: 99 } : row),
    ts.map((row, index) => index === 0 ? { ...row, id: 'extra' } : row)]) assert.throws(() => compare(definition, ts, mutation));
  assert.deepEqual(ts.find(row => row.id === 'request-halt-zero-dispatch'), { id: 'request-halt-zero-dispatch', calls: 0,
    sameValue: false, sameError: false, retainedOutcome: false, code: 'drift_blocked', driftClass: 'semantic', bypasses: 0,
    sameArgs: false, sameThis: false, isAsync: false });
});
