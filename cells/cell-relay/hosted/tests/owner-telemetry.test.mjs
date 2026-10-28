import { test, mock } from 'node:test';
import assert from 'node:assert/strict';
const { admission, emptyCounters, report, ingressUnavailable } = /** @type {typeof import('../owner/telemetry.js')} */ (await import(new URL('../owner/telemetry.ts', import.meta.url).href));

test('logs closed counters and timings without credential material', async () => {
  /** @type {string[]} */
  const logs = [];
  const capture = mock.method(console, 'info', (/** @type {string} */ value) => logs.push(value));
  try {
    const counts = emptyCounters();
    for (const [status, error, expected] of /** @type {const} */ ([
      [401, 'auth_Replay', 'replay'], [401, 'identity_Replay', 'replay'],
      [401, 'identity_replay', 'replay'], [401, 'private assertion DPoP secret', 'rejected'],
      [429, 'identity_capacity', 'capacity'], [503, 'private signing_seed', 'unavailable'],
    ])) {
      const outcome = await admission(Response.json({ error }, { status }));
      assert.equal(outcome, expected);
      counts[outcome]++;
      report(counts, outcome, 1);
    }
    ingressUnavailable();
    assert.equal(logs.length, 7);
    for (const entry of logs) {
      const parsed = JSON.parse(entry);
      assert.ok(Object.keys(parsed).every(key => ['event', 'outcome', 'accepted', 'rejected', 'replay', 'capacity', 'unavailable', 'elapsed_ms'].includes(key)));
      assert.doesNotMatch(entry, /assertion|DPoP|secret|signing_seed|authorization|access_token/u);
    }
    const before = logs.length;
    report(counts, 'accepted', 2);
    assert.equal(logs.length, before);
  } finally { capture.mock.restore(); }
});

test('telemetry never buffers an upstream error body or counts upstream throttling as owner capacity', async () => {
  for (const status of [401, 429, 503]) {
    const response = new Response('customer bytes', { status, headers: { 'x-agindow-status': 'ok' } });
    const clone = mock.method(response, 'clone', () => { throw Error('upstream body inspected'); });
    try {
      assert.equal(await admission(response), 'accepted');
      assert.equal(clone.mock.callCount(), 0);
      assert.equal(response.bodyUsed, false);
    } finally { clone.mock.restore(); }
  }
  assert.equal(await admission(Response.json({ error: 'auth_Limited' }, { status: 429 })), 'rejected');
});
