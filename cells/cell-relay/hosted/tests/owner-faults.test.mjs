import { test, mock } from 'node:test';
import assert from 'node:assert/strict';
import { generateKeyPairSync } from 'node:crypto';
import { performance } from 'node:perf_hooks';
import { createRemoteAuth, guardRemote } from '../../node/dist/esm/index.js';
import { ownerHarness, credential } from './support/owner-harness.mjs';

const endpoint = 'https://guard.invalid/v1/guard';
/** @type {import('../../node/dist/esm/index.js').RemoteEnvelope} */
const input = { tool: 'op', connection: 'c', operation: 'read', args: {}, mode: 'observe' };

test('authority partition, stale routes and lost acknowledgements never dispatch a replay', async () => {
  const f = await ownerHarness();
  try {
    const owner = await f.owner();
    await owner.retained();
    await f.setTime(f.now + 361);
    const issued = await f.exchange('lost-ack');
    assert.equal(issued.status, 200);
    // Drop the acknowledgement, not the consumed assertion. A replacement retains its fence.
    await issued.body?.cancel();
    await f.restart();
    assert.equal((await f.exchange('lost-ack', 'ingress-b')).status, 401);
    const next = await f.exchange('fresh');
    const { access_token: access } = await credential(next);
    const proof = f.signer.proof(access, 'POST', endpoint, f.now);
    const active = await f.owner();
    await active.partition(true);
    assert.equal((await f.guard(access, proof, 'ingress-a')).status, 503);
    assert.equal(f.dispatches, 0);
    await active.partition(false);
    assert.equal((await f.guard(access, proof, 'ingress-b')).status, 200);
    const wrongOwner = await f.owner('foreign');
    assert.equal((await wrongOwner.execute(new Request(endpoint, { method: 'POST' }), 'workload')).status, 403);
    await f.restart();
    assert.equal((await f.guard(access, proof, 'ingress-a')).status, 503);
    assert.equal(f.dispatches, 1);
    assert.equal((await f.exchange('lost-ack')).status, 401);
  } finally { await f.close(); }
});

test('invalid signatures consume no assertion fence and counters contain only closed fields', async () => {
  const f = await ownerHarness();
  try {
    const owner = await f.owner();
    await owner.retained();
    await f.setTime(f.now + 361);
    const wrong = generateKeyPairSync('ed25519');
    assert.equal((await f.exchange('same', 'ingress-a', ['op'], 'workload', wrong.privateKey)).status, 401);
    assert.equal(await owner.retained(), 0);
    assert.equal((await f.exchange('same')).status, 200);
    assert.equal((await f.exchange('same', 'ingress-b')).status, 401);
    await owner.fillLedger(f.now + 300);
    assert.equal((await f.exchange('full')).status, 429);
    await owner.eraseLedger();
    assert.equal((await f.exchange('fault')).status, 503);
    const counters = JSON.parse(await owner.counters());
    assert.deepEqual(counters, { accepted: 1, rejected: 1, replay: 1, capacity: 1, unavailable: 1 });
    assert.deepEqual(Object.keys(counters).sort(), ['accepted', 'capacity', 'rejected', 'replay', 'unavailable']);
  } finally { await f.close(); }
});

test('SDK recovery uses background acquisition and one bypass per unavailable call', async () => {
  const f = await ownerHarness();
  /** @type {import('../../node/dist/esm/index.js').RemoteAuth|undefined} */
  let auth;
  let transport;
  try {
    await (await f.owner()).retained();
    await f.setTime(f.now + 361);
    let acquisitions = 0, fallbacks = 0;
    /** @type {import('../../node/dist/esm/index.js').Event[]} */
    const events = [];
    const session = auth = await createRemoteAuth({ schedule: false, now: () => f.now, acquire: async () => {
      acquisitions++;
      const issued = await f.exchange(`sdk-${acquisitions}`);
      if (!issued.ok) throw Error('unavailable');
      const { access_token: accessToken, expires_at: expiresAt } = await credential(issued);
      return { accessToken, expiresAt };
    }, signer: f.signer });
    transport = mock.method(globalThis, 'fetch', async (/** @type {Parameters<typeof fetch>[0]} */ url, /** @type {RequestInit|undefined} */ init) => { const request = new Request(url, init); return (await f.ingress()).fetch(request.url, { method: request.method, headers: [...request.headers], ...(request.body === null ? {} : { body: await request.text() }) }); });
    const call = () => guardRemote(() => { fallbacks++; return new Response('original'); }, input, { endpoint, auth: session, onEvent: (event) => events.push(event) });
    assert.equal((await call()).status, 200);
    await f.restart();
    assert.equal(await (await call()).text(), 'original');
    assert.equal(acquisitions, 1);
    assert.equal(fallbacks, 1);
    assert.equal(events.filter(event => event.type === 'bypass').length, 1);
    const lostAt = f.now;
    await f.setTime(f.now + 240); // 300s access token schedules refresh 60s before expiry.
    assert.equal(await session.refresh(), true);
    assert.equal((await call()).status, 200);
    assert.ok(f.now - lostAt <= 335);
    // A known replay is a rejection; the SDK never calls the original binding.
    const fixed = session.headers('POST', endpoint);
    const replayAuth = { headers: () => fixed, refresh: () => session.refresh(), close: () => {} };
    const replayCall = () => guardRemote(() => { fallbacks++; return new Response('original'); }, input, { endpoint, auth: replayAuth, onEvent: event => events.push(event) });
    assert.equal((await replayCall()).status, 200);
    assert.equal((await replayCall()).status, 401);
    assert.equal(fallbacks, 1);
    assert.equal(events.filter(event => event.type === 'bypass').length, 1);
    // Empty durable state has a separate 360s quarantine; it cannot promise the warm-owner bound.
    await (await f.owner()).eraseLedger();
    await f.restart();
    const coldAt = f.now;
    assert.equal(await session.refresh(), false);
    assert.equal(await (await call()).text(), 'original');
    assert.equal(acquisitions, 3);
    await f.setTime(coldAt + 359);
    assert.equal(await session.refresh(), false);
    await f.setTime(coldAt + 365);
    assert.equal(await session.refresh(), true);
    assert.equal((await call()).status, 200);
    assert.ok(f.now - coldAt <= 370);
    assert.equal(fallbacks, 2);
    assert.equal(events.filter(event => event.type === 'bypass').length, 2);
  } finally { auth?.close(); transport?.mock.restore(); await f.close(); }
});

test('reports local ingress routing overhead separately from direct owner execution', async (t) => {
  const f = await ownerHarness();
  try {
    const owner = await f.owner();
    await owner.retained();
    await f.setTime(f.now + 361);
    const { access_token: access } = await credential(await f.exchange('timing'));
    /** @type {{direct:number[],routed:number[]}} */
    const samples = { direct: [], routed: [] };
    for (let i = 0; i < 24; i++) {
      for (const route of /** @type {const} */ (['direct', 'routed'])) {
        const request = new Request(endpoint, { method: 'POST', headers: { authorization: `DPoP ${access}`, dpop: f.signer.proof(access, 'POST', endpoint, f.now) }, body: JSON.stringify(input) });
        const start = performance.now();
        const response = route === 'direct' ? await owner.execute(request, 'workload') : await (await f.ingress(i % 2 ? 'ingress-a' : 'ingress-b')).fetch(request.url, { method: 'POST', headers: [...request.headers], body: JSON.stringify(input) });
        assert.equal(response.status, 200);
        await response.text();
        if (i >= 4) samples[route].push(performance.now() - start);
      }
    }
    /** @param {number[]} values @param {number} fraction */
    const p = (values, fraction) => {
      const value = [...values].sort((a, b) => a - b)[Math.ceil(values.length * fraction) - 1];
      if (value === undefined) throw Error('missing timing sample');
      return value;
    };
    const report = { direct_p50_ms: p(samples.direct, .5), direct_p95_ms: p(samples.direct, .95), routed_p50_ms: p(samples.routed, .5), routed_p95_ms: p(samples.routed, .95) };
    t.diagnostic(JSON.stringify({ local_owner_routing: report, routing_delta_p50_ms: report.routed_p50_ms - report.direct_p50_ms, samples: 20, live_qualification: false }));
    assert.equal(f.dispatches, 48);
  } finally { await f.close(); }
});
