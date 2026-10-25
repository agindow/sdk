import { expect, it } from 'vitest';
import express from 'express';
import { createServer } from 'node:http';
import type { AddressInfo } from 'node:net';
import { createAgindow } from '../src/index.js';
import { capture } from '../src/inspection.js';
import type { Verdict } from '../src/types.js';
const pass: Verdict = { halt: false, bypassed: false, class: null, findings: 0, detectors: 0, malformed: false };
it('Express buffers complete JSON end bytes for inspection and preserves them exactly', async () => {
  const seen: string[] = [];
  const sdk = createAgindow({}, async () => ({ begin: () => ({ request: pass, finish(value) {
    seen.push(capture(value).json); return pass;
  }, close() {} }) })); await sdk.ready;
  const app = express(); const raw = ' {"id":9007199254740993} ';
  app.get('/', (_req, res) => { res.type('json').end(raw); });
  const server = createServer(sdk.httpAdapter(app));
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
  try {
    const response = await fetch(`http://127.0.0.1:${(server.address() as AddressInfo).port}/`);
    expect(await response.text()).toBe(raw); expect(seen).toEqual([raw]);
  } finally { await new Promise<void>(resolve => server.close(() => resolve())); }
});
it('Express request halts never execute routes and use structured HTTP feedback', async () => {
  let routes = 0;
  const halt = { ...pass, halt: true, findings: 1, class: 'semantic' as const };
  const sdk = createAgindow({ mode: 'strict' }, async () => ({ begin: () => ({ request: halt, finish: () => pass, close() {} }) })); await sdk.ready;
  const app = express(); app.post('/', (_req, res) => { routes++; res.json({ ok: true }); });
  const server = createServer(sdk.httpAdapter(app, { operation: 'write' }));
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
  try {
    const response = await fetch(`http://127.0.0.1:${(server.address() as AddressInfo).port}/`, { method: 'POST' });
    expect(response.status).toBe(409); expect(await response.json()).toMatchObject({ error: 'drift_halted', drift_class: 'semantic' });
    expect(routes).toBe(0);
  } finally { await new Promise<void>(resolve => server.close(() => resolve())); }
});
