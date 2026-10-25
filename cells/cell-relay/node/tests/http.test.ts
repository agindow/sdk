import { expect, it, vi } from 'vitest';
import fc from 'fast-check';
import { createAgindow, AgindowDriftError } from '../src/index.js';
import { capture } from '../src/inspection.js';
import type { Engine, Verdict } from '../src/types.js';
const pass: Verdict = { halt: false, bypassed: false, class: null, findings: 0, detectors: 0, malformed: false };
it('forwards exact JSON Responses and raw numeric lexemes while inspecting bounded copies', async () => {
  const seen: string[] = [];
  const engine: Engine = { begin(input) {
    seen.push(capture(input).json);
    return { request: pass, finish(value) { seen.push(capture(value).json); return pass; }, close() {} };
  } };
  const sdk = createAgindow({}, async () => engine); await sdk.ready;
  await fc.assert(fc.asyncProperty(fc.string(), async text => {
    const raw = ` {"id":9007199254740993,"s":${JSON.stringify(text)}} `;
    const response = new Response(raw, { headers: { 'content-type': 'application/json', 'content-length': String(Buffer.byteLength(raw)) } });
    const upstream = vi.fn(async () => response);
    const fetch = sdk.httpAdapter(upstream);
    expect(await fetch()).toBe(response); expect(response.bodyUsed).toBe(false);
    expect(await response.text()).toBe(raw); expect(upstream).toHaveBeenCalledTimes(1);
    expect(seen.at(-1)).toBe(raw);
  }), { numRuns: 50 });
});
it('never clones or consumes chunked, non-JSON, unbounded or oversized responses', async () => {
  for (const headers of [
    { 'content-type': 'text/plain', 'content-length': '2' },
    { 'content-type': 'application/json', 'transfer-encoding': 'chunked', 'content-length': '2' },
    { 'content-type': 'application/json' },
    { 'content-type': 'application/json', 'content-length': '9999999' },
  ]) {
    const response = new Response('{}', { headers }); const clone = vi.spyOn(response, 'clone');
    const finish = vi.fn(() => pass);
    const sdk = createAgindow({}, async () => ({ begin: () => ({ request: pass, finish, close() {} }) })); await sdk.ready;
    expect(await sdk.httpAdapter(async () => response)()).toBe(response);
    expect(clone).not.toHaveBeenCalled(); expect(finish).not.toHaveBeenCalled(); expect(response.bodyUsed).toBe(false);
  }
});
it('halts before dispatch and retains completed Response and replay warning on response halts', async () => {
  for (const phase of ['request', 'response']) {
    const halt = { ...pass, halt: true, class: 'semantic' as const, findings: 1 };
    const sdk = createAgindow({ mode: 'strict' }, async () => ({ begin: () => ({
      request: phase === 'request' ? halt : pass, finish: () => halt, close() {},
    }) })); await sdk.ready;
    const response = new Response('{}', { headers: { 'content-type': 'application/json', 'content-length': '2' } });
    const fetch = vi.fn(async () => response);
    try { await sdk.httpAdapter(fetch)(); throw Error('missing halt'); }
    catch (error) {
      expect(error).toBeInstanceOf(AgindowDriftError);
      if (error instanceof AgindowDriftError && phase === 'response') {
        expect(error.outcome).toBe(response); expect(error.hint).toContain('do not blindly replay');
      }
    }
    expect(fetch).toHaveBeenCalledTimes(phase === 'request' ? 0 : 1);
  }
});
it('contains response capture faults and retains the exact completed outcome', async () => {
  const response = new Response('{}');
  Object.defineProperty(response, 'headers', { get() { throw Error('capture fixture'); } });
  const sdk = createAgindow({}, async () => ({ begin: () => ({ request: pass, finish: () => pass, close() {} }) }));
  await sdk.ready;
  const upstream = vi.fn(async () => response);
  expect(await sdk.httpAdapter(upstream)()).toBe(response);
  expect(upstream).toHaveBeenCalledTimes(1);
  expect(sdk.recent().filter(event => event.type === 'bypass')).toHaveLength(1);
  expect(response.bodyUsed).toBe(false);
});
it('recomputes request framing only for an applied verified raw-body repair', async () => {
  const body = '{"new":12345}';
  const repair: Verdict = { ...pass, findings: 1, class: 'shape', confidence: .99,
    repair: { verified: true, value: Buffer.from(body), changes: [{ path: '/old', kind: 'field_renamed', to: '/new' }] } };
  const sdk = createAgindow({ mode: 'repair' }, async () => ({ begin: () => ({ request: repair, finish: () => pass, close() {} }) }));
  await sdk.ready;
  const upstream = vi.fn(async (_url: string, _init: RequestInit) => new Response('{}'));
  const init = { method: 'POST', body: '{"old":1}', headers: { 'content-length': '9', 'content-md5': 'stale', 'if-match': 'resource-version', 'x-customer': 'keep' } };
  await sdk.httpAdapter(upstream)('https://example.test', init);
  const sent = upstream.mock.calls[0]?.[1];
  expect(Buffer.from(sent?.body as Uint8Array).toString()).toBe(body);
  const headers = new Headers(sent?.headers);
  expect(headers.get('content-length')).toBe(String(Buffer.byteLength(body)));
  expect(headers.has('content-md5')).toBe(false);
  expect(headers.get('if-match')).toBe('resource-version');
  expect(headers.get('x-customer')).toBe('keep');
  expect(init.headers['content-length']).toBe('9');
});
it('refuses raw request repair when the original body or encoding is unavailable', async () => {
  for (const init of [{ method: 'POST' }, { method: 'POST', body: '{}', headers: { 'content-encoding': 'gzip' } }]) {
    const repair: Verdict = { ...pass, class: 'shape', confidence: .99, repair: { verified: true, value: Buffer.from('{"new":1}'), changes: [] } };
    const sdk = createAgindow({ mode: 'repair' }, async () => ({ begin: () => ({ request: repair, finish: () => pass, close() {} }) }));
    await sdk.ready;
    const upstream = vi.fn(async (_url: string, _init: RequestInit) => new Response('{}'));
    await sdk.httpAdapter(upstream)('https://example.test', init);
    expect(upstream.mock.calls[0]?.[1]).toBe(init);
    expect(sdk.recent().filter(event => event.type === 'repair')).toHaveLength(0);
    expect(sdk.recent().filter(event => event.type === 'bypass')).toHaveLength(1);
  }
});

it('wraps global fetch without network and preserves its complete Response', async () => {
  const response = new Response('{}', { headers: { 'content-type': 'application/json', 'content-length': '2' } });
  const upstream = vi.spyOn(globalThis, 'fetch').mockResolvedValue(response);
  try {
    const finish = vi.fn(() => pass);
    const sdk = createAgindow({}, async () => ({ begin: () => ({ request: pass, finish, close() {} }) })); await sdk.ready;
    const init = { method: 'GET' };
    expect(await sdk.wrap(globalThis.fetch)('https://example.test', init)).toBe(response);
    expect(upstream).toHaveBeenCalledExactlyOnceWith('https://example.test', init);
    expect(finish).toHaveBeenCalledOnce(); expect(response.bodyUsed).toBe(false);
  } finally { upstream.mockRestore(); }
});
it('exposes changed response bytes honestly and discards stale body integrity headers', async () => {
  const body = '{"new":12345}';
  const repair: Verdict = { ...pass, findings: 1, class: 'shape', confidence: .99,
    repair: { verified: true, value: Buffer.from(body), changes: [{ path: '/old', kind: 'field_renamed', to: '/new' }] } };
  const sdk = createAgindow({ mode: 'repair' }, async () => ({ begin: () => ({ request: pass, finish: () => repair, close() {} }) }));
  await sdk.ready;
  const original = new Response('{"old":1}', { status: 201, statusText: 'Created', headers: {
    'content-type': 'application/json', 'content-length': '9', etag: 'stale', 'content-md5': 'stale', digest: 'stale', 'x-customer': 'keep',
  } });
  const upstream = vi.fn(async () => original);
  const result = await sdk.httpAdapter(upstream)();
  expect(result).not.toBe(original); expect(await result.text()).toBe(body);
  expect(result.status).toBe(201); expect(result.statusText).toBe('Created');
  expect(result.headers.get('content-length')).toBe(String(Buffer.byteLength(body)));
  for (const key of ['etag', 'content-md5', 'digest']) expect(result.headers.has(key)).toBe(false);
  expect(result.headers.get('x-customer')).toBe('keep');
  expect(original.headers.get('digest')).toBe('stale'); expect(original.bodyUsed).toBe(false);
  expect(sdk.recent().filter(event => event.type === 'repair')).toMatchObject([{ side: 'response', byte_identical: false }]);
  expect(upstream).toHaveBeenCalledOnce();
});
