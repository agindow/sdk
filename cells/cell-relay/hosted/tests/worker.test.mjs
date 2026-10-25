import { test } from 'node:test';
import assert from 'node:assert/strict';
import { Miniflare } from 'miniflare';
import { createHash } from 'node:crypto';
import { fileURLToPath } from 'node:url';
test('real workerd authenticates, enforces scope/quota and preserves raw upstream bytes', async () => {
  let dispatches = 0;
  const token = 'local-worker-fixture';
  const runtime = new Miniflare({ modules: true, modulesRules: [{ type: 'CompiledWasm', include: ['**/*.wasm'] }], scriptPath: fileURLToPath(new URL('../../crates/http-host/build/index.js', import.meta.url)),
    compatibilityDate: '2026-08-06', bindings: {
      GUARD_KEYS: JSON.stringify([{ digest: createHash('sha256').update(token).digest('hex'), tenant: 't', routes: ['c/op'], plan: 'local', limit: 2, window_seconds: 86400 }]),
      GUARD_CONNECTIONS: JSON.stringify({ 'c/op': { tenant: 't', url: 'https://vendor.invalid/action', method: 'POST', operation: 'write' } }) },
    outboundService: async (/** @type {Request} */ request) => {
      dispatches++;
      assert.equal(request.headers.get('idempotency-key'), 'same-key');
      assert.equal(await request.text(), '{"n":1e400,"n":2}');
      return new Response('{ "raw":1e400 }', { status: 201, headers: { 'x-agindow-status': 'forged', 'content-type': 'application/json' } });
    } });
  const input = '{"tool":"op","connection":"c","operation":"write","args":{"n":1e400,"n":2},"mode":"repair","idempotency_key":"same-key"}';
  try {
    const send = (authorization = `Bearer ${token}`) => runtime.dispatchFetch('https://guard.invalid/v1/guard', { method: 'POST', headers: { authorization }, body: input });
    assert.equal((await send('Bearer wrong')).status, 401);
    assert.equal(dispatches, 0);
    const response = await send();
    assert.equal(response.status, 201); assert.equal(await response.text(), '{ "raw":1e400 }');
    assert.notEqual(response.headers.get('x-agindow-status'), 'forged');
    assert.equal(response.headers.get('x-agindow-quota-scope'), 'isolate');
    assert.equal(response.headers.get('x-agindow-latency-ms'), 'unavailable');
    assert.equal(response.headers.get('x-agindow-latency-clock'), 'unavailable');
    assert.equal((await send()).status, 201);
    const limited = await send(); assert.equal(limited.status, 429);
    assert.equal(limited.headers.get('x-ratelimit-remaining'), '0');
    assert.equal(limited.headers.get('x-agindow-quota-scope'), 'isolate'); assert.equal(dispatches, 2);
  } finally { await runtime.dispose(); }
});
test('real Worker request/response policy halts retain six fields, scope and completed outcomes', async () => {
  for (const side of ['request', 'response']) {
    let calls = 0;
    const selection = { tenant: 't', connection: 'c', vendor: 'v', operation: 'op', segment: 's',
      fields: [{ side, path: '/value', baseline: Array.from({ length: 128 }, () => '{"value":1}'),
        live: Array.from({ length: 128 }, () => '{"value":100}'), enumeration: true }] };
    const runtime = new Miniflare({ modules: true, modulesRules: [{ type: 'CompiledWasm', include: ['**/*.wasm'] }],
      scriptPath: fileURLToPath(new URL('../../crates/http-host/build/index.js', import.meta.url)), compatibilityDate: '2026-08-06', bindings: {
        GUARD_KEYS: JSON.stringify([{ digest: createHash('sha256').update('key').digest('hex'), tenant: 't', routes: ['c/op'], plan: 'fixture', limit: 10, window_seconds: 60 }]),
        GUARD_CONNECTIONS: JSON.stringify({ 'c/op': { tenant: 't', url: 'https://vendor.invalid/action', method: 'POST', operation: 'write', selection } }) },
      outboundService: async () => { calls++; return new Response(' {"value":100}\n', { status: 201, headers: { 'content-length': '15' } }); } });
    try {
      for (const mode of ['repair','strict']) {
        const response = await runtime.dispatchFetch('https://guard.invalid/v1/guard', { method: 'POST', headers: { authorization: 'Bearer key' },
          body: JSON.stringify({ tool: 'op', connection: 'c', operation: 'write', args: { value: 100 }, mode, idempotency_key: 'same' }) });
        assert.equal(response.status, 409);
        const feedback = /** @type {Record<string, string>} */ (await response.json());
        assert.deepEqual(Object.keys(feedback).sort(), ['audit','drift_class','drift_id','error','hint','tool']);
        assert.equal(feedback['error'], mode === 'strict' ? 'drift_halted' : 'drift_blocked');
        const audit = await runtime.dispatchFetch(new URL(String(feedback['audit']), 'https://guard.invalid'), { headers: { authorization: 'Bearer key' } });
        assert.equal(audit.status, 200);
        if (side === 'response') {
          assert.match(String(feedback['hint']), /replay/i);
          const location = response.headers.get('x-agindow-outcome'); assert.ok(location);
          const retained = await runtime.dispatchFetch(new URL(location, 'https://guard.invalid'), { headers: { authorization: 'Bearer key' } });
          assert.equal(retained.status, 201); assert.equal(retained.headers.get('x-agindow-retained-outcome'), 'true');
          assert.equal(await retained.text(), ' {"value":100}\n');
          assert.equal((await runtime.dispatchFetch(new URL(location, 'https://guard.invalid'), { headers: { authorization: 'Bearer wrong' } })).status, 404);
        }
      }
      assert.equal(calls, side === 'request' ? 0 : 2);
    } finally { await runtime.dispose(); }
  }
});
test('registry binding is an explicit administrative probe and is never needed to guard a call', async () => {
  const runtime = new Miniflare({ workers: [
    { name: 'guard', modules: true, modulesRules: [{ type: 'CompiledWasm', include: ['**/*.wasm'] }],
      scriptPath: fileURLToPath(new URL('../../crates/http-host/build/index.js', import.meta.url)), compatibilityDate: '2026-08-06',
      bindings: { REGISTRY_PROBE_KEY: 'admin', REGISTRY_HEALTH_URL: 'https://registry.invalid/healthz' }, serviceBindings: { REGISTRY_MTLS: 'registry-stub' } },
    { name: 'registry-stub', modules: true, script: "export default { fetch(request) { return new Response(null, {status: new URL(request.url).pathname === '/healthz' ? 204 : 404}); } };" },
  ] });
  try {
    assert.equal((await runtime.dispatchFetch('https://guard.invalid/internal/registry-probe', { method: 'POST' })).status, 401);
    const response = await runtime.dispatchFetch('https://guard.invalid/internal/registry-probe', { method: 'POST', headers: { authorization: 'Bearer admin' } });
    assert.equal(response.status, 200);
    assert.deepEqual(await response.json(), { mtls_binding: 'REGISTRY_MTLS', registry_status: 204 });
  } finally { await runtime.dispose(); }
});
test('overhead header excludes incoming upload and upstream network waits', async () => {
  const runtime = new Miniflare({ modules: true, modulesRules: [{ type: 'CompiledWasm', include: ['**/*.wasm'] }],
    scriptPath: fileURLToPath(new URL('../../crates/http-host/build/index.js', import.meta.url)), compatibilityDate: '2026-08-06', bindings: { AGINDOW_LATENCY_CLOCK: 'workerd-local',
      GUARD_KEYS: JSON.stringify([{ digest: createHash('sha256').update('key').digest('hex'), tenant: 't', routes: ['c/op'], plan: 'fixture', limit: 10, window_seconds: 60 }]),
      GUARD_CONNECTIONS: JSON.stringify({ 'c/op': { tenant: 't', url: 'https://vendor.invalid/action', method: 'POST', operation: 'read' } }) },
    outboundService: async () => { await new Promise(resolve => setTimeout(resolve, 100)); return new Response('{}'); } });
  try {
    await runtime.ready;
    await runtime.dispatchFetch('https://guard.invalid/healthz');
    const input = JSON.stringify({ tool: 'op', connection: 'c', operation: 'read', args: {}, mode: 'observe' });
    const body = new ReadableStream({ start(controller) {
      controller.enqueue(new TextEncoder().encode(input.slice(0, 20)));
      setTimeout(() => { controller.enqueue(new TextEncoder().encode(input.slice(20))); controller.close(); }, 250);
    } });
    const response = await runtime.dispatchFetch('https://guard.invalid/v1/guard', { method: 'POST', headers: { authorization: 'Bearer key' }, body, duplex: 'half' });
    assert.equal(response.status, 200); assert.equal(await response.text(), '{}');
    const overhead = Number(response.headers.get('x-agindow-latency-ms'));
    assert.ok(Number.isFinite(overhead) && overhead < 100, `network waits counted as engine overhead: ${overhead}`);
  } finally { await runtime.dispose(); }
});
