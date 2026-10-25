import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createAgindow, AgindowDriftError } from '../dist/esm/index.js';

test('release guest enqueues schema-validated artifacts once and retains stable heads across failed delivery', async () => {
  const sdk = createAgindow({ audit: { tenant: 'tenant', capacity: 2 } }); await sdk.ready;
  const value = {}; const error = Error('upstream');
  assert.equal(sdk.guard(() => value, { tool: 'vendor.read' })(), value);
  assert.throws(sdk.guard(() => { throw error; }, { tool: 'vendor.error' }), e => e === error);
  assert.equal(sdk.guard(() => value)(), value); // rejected newest at capacity
  /** @type {string[]} */ const attempts = [];
  const failed = await sdk.drainAudit(artifact => { attempts.push(artifact.id); throw Error('offline'); });
  assert.deepEqual(failed, { delivered: 0, pending: 2, dropped: 1, available: true });
  /** @type {import('../src/audit.js').AuditRecord[]} */ const delivered = [];
  const drained = await sdk.drainAudit(artifact => { delivered.push(artifact); });
  assert.deepEqual(drained, { delivered: 2, pending: 0, dropped: 1, available: true });
  assert.equal(attempts[0], delivered[0]?.id);
  assert.deepEqual(delivered.map(a => a['tool']), ['vendor.read', 'vendor.error']);
  assert.ok(delivered.every(a => a['decision'] === 'ok'));
});
test('release guest composes request halts with zero dispatch and budget bypasses with one dispatch', async () => {
  const selection = { tenant: 'tenant', connection: 'c', vendor: 'v', operation: 'op', segment: 's',
    fields: [{ side: /** @type {const} */ ('request'), path: '/value', baseline: Array(128).fill('{"value":1}'), live: Array(128).fill('{"value":100}'), enumeration: true }] };
  for (const work of [200_000_000, 0]) {
    const sdk = createAgindow({ mode: 'repair', audit: { tenant: 'tenant' }, selection, work }); await sdk.ready;
    let calls = 0; const value = { value: 100 };
    const run = sdk.guard(() => { calls++; return value; }, { tool: 'op', operation: 'write' });
    if (work) assert.throws(run, AgindowDriftError); else assert.equal(run(), value);
    assert.equal(calls, work ? 0 : 1);
    /** @type {import('../src/audit.js').AuditRecord[]} */ const records = [];
    assert.equal((await sdk.drainAudit(a => { records.push(a); })).delivered, 1);
    assert.equal(records[0]?.['decision'], work ? 'blocked' : 'bypassed');
  }
});
test('disabled publication does not invoke a sink or enqueue', async () => {
  const sdk = createAgindow(); await sdk.ready; sdk.guard(() => 42)();
  assert.equal((await sdk.drainAudit(() => { throw Error('must stay disabled'); })).available, false);
});
test('unavailable host inspection still enqueues one bypass artifact without changing the original result', async () => {
  const selection = { tenant: 'tenant', connection: 'c', vendor: 'v', operation: 'op', segment: 's', fields: [{
    side: /** @type {const} */ ('request'), path: '/value', baseline: ['{"value":1}'], live: ['{"value":1}'], enumeration: true }] };
  const sdk = createAgindow({ audit: { tenant: 'tenant' }, selection }); await sdk.ready;
  const cyclic = {}; Object.assign(cyclic, { self: cyclic }); const result = {};
  assert.equal(sdk.guard((/** @type {unknown} */ _input) => result, { tool: 'op' })(cyclic), result);
  /** @type {import('../src/audit.js').AuditRecord[]} */ const records = [];
  assert.equal((await sdk.drainAudit(value => { records.push(value); })).delivered, 1);
  assert.equal(records[0]?.['decision'], 'bypassed');
});
test('unavailable response projection enqueues a bypass while retaining the completed result', async () => {
  const selection = { tenant: 'tenant', connection: 'c', vendor: 'v', operation: 'op', segment: 's', fields: [{
    side: /** @type {const} */ ('response'), path: '/value', baseline: ['{"value":1}'], live: ['{"value":1}'], enumeration: true }] };
  const sdk = createAgindow({ audit: { tenant: 'tenant' }, selection }); await sdk.ready;
  const result = new Map([['value', 1]]);
  assert.equal(sdk.guard(() => result, { tool: 'op' })(), result);
  /** @type {import('../src/audit.js').AuditRecord[]} */ const records = [];
  assert.equal((await sdk.drainAudit(value => { records.push(value); })).delivered, 1);
  assert.equal(records[0]?.['decision'], 'bypassed');
});
