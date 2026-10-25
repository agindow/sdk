import { afterEach, expect, it } from 'vitest';
import { createServer } from 'node:http';
import type { Server, RequestListener } from 'node:http';
import { guardRemote } from '../src/remote.js';
import { AgindowDriftError } from '../src/types.js';
import type { RemoteEnvelope } from '../src/remote.js';
const servers: Server[] = [];
afterEach(async () => { for (const server of servers.splice(0)) { server.closeAllConnections(); await new Promise<void>(resolve => server.close(() => resolve())); } });
async function endpoint(handler: RequestListener): Promise<string> {
  const server = createServer(handler); servers.push(server);
  await new Promise<void>((resolve, reject) => { server.once('error', reject); server.listen(0, '127.0.0.1', resolve); });
  const address = server.address();
  if (address === null || typeof address === 'string') throw Error('missing port');
  return `http://127.0.0.1:${address.port}/v1/guard`;
}
const envelope: RemoteEnvelope = { tool: 'vendor.call', connection: 'conn_fixture', operation: 'read', args: {}, mode: 'repair' };
for (const operation of ['read', 'write', 'unknown'] as const) {
  it(`fails open on every 5xx for ${operation} with one event and original identity`, async () => {
    let status = 500; let requests = 0;
    const url = await endpoint((_request, response) => { requests++; response.writeHead(status).end('internal'); });
    for (status = 500; status <= 599; status++) {
      const events: string[] = []; const original = {}; let calls = 0;
      const input = { ...envelope, operation, idempotency_key: 'same-key' };
      expect(await guardRemote(received => { expect(received).toBe(input); calls++; return original; }, input,
        { endpoint: url, onEvent: event => events.push(event.type) })).toBe(original);
      expect(calls).toBe(1); expect(events).toEqual(['bypass']);
    }
    expect(requests).toBe(100);
  });
}
for (const fault of ['refused', 'reset', 'timeout', 'feedback-timeout'] as const) {
  it(`preserves the original error on ${fault}`, async () => {
    const url = await endpoint((request, response) => {
      if (fault === 'reset') request.socket.destroy();
      if (fault === 'feedback-timeout') { response.writeHead(409); response.write('{'); }
    });
    if (fault === 'refused') await new Promise<void>(resolve => servers[0]?.close(() => resolve()));
    const error = Error('original'); const events: string[] = [];
    await expect(guardRemote(() => { throw error; }, envelope, { endpoint: url, timeoutMs: 30,
      onEvent: event => events.push(event.type) })).rejects.toBe(error);
    expect(events).toEqual(['bypass']);
  });
}
it('returns successful response bytes untouched without calling fallback', async () => {
  const bytes = ' {"n":1e400, "n":2}\n';
  const url = await endpoint((_request, response) => response.writeHead(201).end(bytes));
  let calls = 0;
  const result = await guardRemote(() => { calls++; return null; }, envelope, { endpoint: url });
  expect(result).toBeInstanceOf(Response);
  if (result instanceof Response) { expect(result.status).toBe(201); expect(await result.text()).toBe(bytes); }
  expect(calls).toBe(0);
});
for (const mode of ['strict', 'repair'] as const) {
  it(`honors exact six-field ${mode} halt without fallback`, async () => {
    const feedback = { error: mode === 'strict' ? 'drift_halted' : 'drift_blocked', tool: envelope.tool,
      drift_class: 'semantic', drift_id: 'd_fixture', hint: 'Review evidence.', audit: 'aud_fixture' };
    const url = await endpoint((_request, response) => response.writeHead(409).end(JSON.stringify(feedback)));
    let calls = 0;
    await expect(guardRemote(() => { calls++; }, { ...envelope, mode }, { endpoint: url })).rejects.toMatchObject({
      name: 'AgindowDriftError', code: feedback.error, driftId: 'd_fixture',
    });
    expect(calls).toBe(0);
  });
}
it('does not turn malformed or mismatched policy feedback into an AGINDOW halt', async () => {
  const url = await endpoint((_request, response) => response.writeHead(409).end('{"error":"drift_blocked"}'));
  const original = {}; expect(await guardRemote(() => original, envelope, { endpoint: url })).toBe(original);
});
it('rejects a write without a deduplication binding before transport', async () => {
  await expect(guardRemote(() => null, { ...envelope, operation: 'write' }, { endpoint: 'http://127.0.0.1:1' }))
    .rejects.toMatchObject({ name: 'AgindowSetupError' });
});
it('retains a completed response halt outcome through the same-origin recovery handle', async () => {
  const bytes = ' {"value":1e400}\n';
  const url = await endpoint((request, response) => {
    if (request.url === '/v1/outcome/aud_fixture') response.writeHead(201).end(bytes);
    else response.writeHead(409, { 'x-agindow-drift-side': 'response', 'x-agindow-outcome': '/v1/outcome/aud_fixture' })
      .end(JSON.stringify({ error: 'drift_blocked', tool: envelope.tool, drift_class: 'semantic', drift_id: 'd_fixture', hint: 'Review.', audit: 'aud_fixture' }));
  });
  let calls = 0;
  try {
    await guardRemote(() => { calls++; }, envelope, { endpoint: url }); throw Error('missing halt');
  } catch (error) {
    expect(error).toBeInstanceOf(Error);
    if (!(error instanceof AgindowDriftError)) throw error;
    expect(error.phase).toBe('response'); expect(error.hint).toContain('do not blindly replay');
    expect(error.outcome).toBeInstanceOf(Response);
    if (error.outcome instanceof Response) { expect(error.outcome.status).toBe(201); expect(await error.outcome.text()).toBe(bytes); }
  }
  expect(calls).toBe(0);
});
it('preserves an upstream 409 stamped as passthrough without dispatching fallback', async () => {
  const url = await endpoint((_request, response) => response.writeHead(409, { 'x-agindow-status': 'ok' }).end('upstream conflict'));
  let calls = 0;
  const result = await guardRemote(() => { calls++; }, envelope, { endpoint: url });
  expect(result).toBeInstanceOf(Response);
  if (result instanceof Response) { expect(result.status).toBe(409); expect(await result.text()).toBe('upstream conflict'); }
  expect(calls).toBe(0);
});
it('preserves a completed upstream 5xx instead of dispatching the operation again', async () => {
  const url = await endpoint((_request, response) => response.writeHead(503, { 'x-agindow-status': 'bypassed' }).end('original upstream failure'));
  const events: string[] = []; let calls = 0;
  const result = await guardRemote(() => { calls++; }, { ...envelope, operation: 'write', idempotency_key: 'same' },
    { endpoint: url, onEvent: event => events.push(event.type) });
  expect(result).toBeInstanceOf(Response);
  if (result instanceof Response) { expect(result.status).toBe(503); expect(await result.text()).toBe('original upstream failure'); }
  expect(calls).toBe(0); expect(events).toEqual(['bypass']);
});
