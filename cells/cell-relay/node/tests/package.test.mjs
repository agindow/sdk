import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { createServer } from 'node:http';
import { once } from 'node:events';
import { createAgindow, AgindowDriftError } from '../dist/esm/index.js';
const require = createRequire(import.meta.url);
/** @param {'request' | 'response'} side @returns {import('../src/types.js').StatisticalSelection} */
const selection = side => ({ tenant: 't', connection: 'c', vendor: 'v', operation: 'op', segment: 's',
  fields: [{ side, path: '/value', baseline: Array(128).fill('{"value":1}'),
    live: Array(128).fill('{"value":100}'), enumeration: true }] });

test('ESM and CJS execute the real packaged guest without warming bypass after ready', async () => {
  for (const module of [{ createAgindow }, /** @type {{createAgindow: typeof createAgindow}} */ (require('../dist/cjs/index.js'))]) {
    /** @type {import('../src/types.js').Event[]} */
    const events = [];
    const sdk = module.createAgindow({ onEvent: e => events.push(e) });
    await sdk.ready;
    const original = {};
    assert.equal(sdk.wrap({ run: () => original }).run(), original);
    assert.deepEqual(events, []);
  }
});
test('real statistical request halt sends zero upstream calls; observe leaves value untouched', async () => {
  /** @type {import('../src/types.js').Event[]} */
    const events = [];
  const sdk = createAgindow({ mode: 'repair', selection: selection('request'), onEvent: e => events.push(e) });
  await sdk.ready;
  let calls = 0;
  const tool = sdk.guard(/** @param {{ value: number }} value */ (value) => { calls++; return value; }, { operation: 'write', tool: 'op' });
  assert.throws(() => tool({ value: 100 }), error => {
    assert.ok(error instanceof AgindowDriftError);
    assert.equal(error.code, 'drift_blocked');
    assert.equal(error.phase, 'request');
    assert.equal(error.verdict.detectors, 5);
    return true;
  });
  assert.equal(calls, 0);
  sdk.configure({ mode: 'observe' });
  const original = { value: 100 };
  assert.equal(tool(original), original);
  assert.equal(calls, 1);
  assert.equal(events.filter(e => e.type === 'bypass').length, 0);
});
test('response policy retains completed result, async snapshot and one upstream attempt', async () => {
  const sdk = createAgindow({ mode: 'observe', selection: selection('response') });
  await sdk.ready;
  /** @type {((value: object) => void) | undefined} */
  let release;
  let calls = 0;
  const original = { value: 100 };
  const wrapped = sdk.guard(() => { calls++; return new Promise(resolve => { release = resolve; }); },
    { operation: 'write', tool: 'op' });
  const pending = wrapped();
  sdk.configure({ mode: 'strict' });
  if (release === undefined) throw Error('missing pending call');
  release(original);
  assert.equal(await pending, original);
  const next = wrapped(); release(original);
  await assert.rejects(next, error => {
    assert.ok(error instanceof AgindowDriftError);
    assert.equal(error.mode, 'strict');
    assert.equal(error.outcome, original);
    assert.match(error.hint, /do not blindly replay/);
    return true;
  });
  assert.equal(calls, 2);
});
test('exhausted guest budget bypasses once and preserves exact outcomes and errors', async () => {
  /** @type {import('../src/types.js').Event[]} */
    const events = [];
  const sdk = createAgindow({ mode: 'strict', work: 0, selection: selection('request'), onEvent: e => events.push(e) });
  await sdk.ready;
  const value = {};
  let calls = 0;
  assert.equal(sdk.guard(() => { calls++; return value; }, { operation: 'write', tool: 'op' })(), value);
  assert.equal(calls, 1);
  assert.equal(events.filter(e => e.type === 'bypass').length, 1);
  const error = {};
  await assert.rejects(sdk.guard(() => Promise.reject(error), { tool: 'op' })(), e => e === error);
});
test('opaque response streams are never consumed; later JSON inspection remains available', async () => {
  /** @type {import('../src/types.js').Event[]} */
    const events = [];
  const sdk = createAgindow({ selection: selection('response'), onEvent: e => events.push(e) });
  await sdk.ready;
  const response = new Response('stream');
  assert.equal(sdk.guard(() => response, { tool: 'op' })(), response);
  assert.equal(response.bodyUsed, false);
  const value = { value: 100 };
  assert.equal(sdk.guard(() => value, { tool: 'op' })(), value);
  assert.equal(events.filter(e => e.type === 'bypass').length, 1);
  assert.ok(events.some(e => e.verdict?.detectors === 5));
});
test('fetch/undici-based and axios-style promise tools run through a local HTTP call', async () => {
  let requests = 0;
  const server = createServer((_request, response) => { requests++; response.end('{"ok":true}'); });
  server.listen(0, '127.0.0.1'); await once(server, 'listening');
  const address = server.address();
  if (address === null || typeof address === 'string') throw Error('missing listener');
  const url = `http://127.0.0.1:${address.port}`;
  try {
    /** @type {import('../src/types.js').Event[]} */
    const events = [];
    const sdk = createAgindow({ onEvent: e => events.push(e) }); await sdk.ready;
    // A mixed HTTP/non-HTTP function map is deliberately ambiguous under
    // TI-31h. Select tools explicitly; the calls still use the packaged guest.
    const tools = sdk.toolAdapter({
      fetch: () => fetch(url),
      axiosStyle: () => fetch(url).then(async response => ({ status: response.status, data: await response.json() })),
    });
    const response = await tools.fetch();
    assert.equal(response.bodyUsed, false);
    assert.deepEqual(await response.json(), { ok: true });
    assert.deepEqual(await tools.axiosStyle(), { status: 200, data: { ok: true } });
    assert.equal(requests, 2);
    assert.deepEqual(events, []);
  } finally { server.closeAllConnections(); await new Promise(resolve => server.close(resolve)); }
});
test('generated garbage-collection cleanup cannot re-enter a retired guest', async () => {
  const original = globalThis.FinalizationRegistry;
  try {
    class DisabledFinalizer {
      constructor() { throw Error('generated finalizer must be disabled'); }
    }
    Reflect.set(globalThis, 'FinalizationRegistry', DisabledFinalizer);
    /** @type {import('../src/types.js').Event[]} */
    const events = [];
    const sdk = createAgindow({ onEvent: e => events.push(e) }); await sdk.ready;
    assert.equal(sdk.guard(() => 42)(), 42);
    assert.deepEqual(events, []);
  } finally { Reflect.set(globalThis, 'FinalizationRegistry', original); }
});
test('an unrelated tool never uses a selected endpoint baseline', async () => {
  /** @type {import('../src/types.js').Event[]} */
  const events = [];
  const sdk = createAgindow({ mode: 'repair', selection: selection('request'), onEvent: e => events.push(e) });
  await sdk.ready;
  const value = { value: 100 };
  assert.equal(sdk.guard(() => value, { tool: 'different', operation: 'write' })(), value);
  assert.equal(events.filter(e => e.type === 'bypass').length, 1);
  assert.throws(() => sdk.guard(() => value, { tool: 'op', operation: 'write' })(), AgindowDriftError);
});
