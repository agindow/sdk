import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createServer, request } from 'node:http';
import { spawn } from 'node:child_process';
import { mkdtempSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { guardRemote } from '../../node/dist/esm/remote.js';
import { AgindowDriftError } from '../../node/dist/esm/types.js';
const binary = process.env.AGINDOW_SIDECAR_BIN ?? fileURLToPath(new URL('../../../../target/debug/agindow-sidecar', import.meta.url));
/** @param {Parameters<typeof createServer>[1]} handler */
async function vendor(handler) {
  const server = createServer(handler); server.listen(0, '127.0.0.1');
  await new Promise((resolve, reject) => { server.once('listening', resolve); server.once('error', reject); });
  const address = server.address(); if (address === null || typeof address === 'string') throw Error('listener missing');
  return { url: `http://127.0.0.1:${address.port}`, async close() { server.closeAllConnections(); await new Promise(resolve => server.close(resolve)); } };
}
/** @param {string} url @param {{side?:'request'|'response',work?:number}} [options] */
async function sidecar(url, options = {}) {
  const directory = mkdtempSync(join(tmpdir(), 'agindow-sidecar-')); const socket = join(directory, 'guard.sock');
  const selection = options.side === undefined ? null : { tenant: 'tenant', connection: 'conn', vendor: 'v', operation: 'op', segment: 's',
    fields: [{ side: options.side, path: '/value', baseline: Array(128).fill('{"value":1}'), live: Array(128).fill('{"value":100}'), enumeration: true }] };
  const child = spawn(binary, [], { env: { ...process.env, AGINDOW_BIND: '127.0.0.1:0', AGINDOW_SOCKET: socket,
    AGINDOW_WORK: String(options.work ?? 200_000_000), AGINDOW_CONNECTIONS: JSON.stringify({ 'conn/op': { url, method: 'POST', operation: 'write', selection } }) }, stdio: ['ignore','pipe','pipe'] });
  let errors = ''; child.stderr.on('data', bytes => { errors += String(bytes); });
  const address = await new Promise((resolve, reject) => {
    const timeout = setTimeout(() => { child.kill('SIGKILL'); reject(Error(`sidecar startup timed out: ${errors}`)); }, 120_000);
    child.once('error', error => { clearTimeout(timeout); reject(error); });
    child.once('exit', code => { clearTimeout(timeout); reject(Error(`sidecar exited ${code}: ${errors}`)); });
    let text = ''; child.stdout.on('data', bytes => { text += String(bytes); if (!text.includes('\n')) return;
      try { const event = JSON.parse(text.split('\n')[0] ?? ''); if (event.event === 'registered') { clearTimeout(timeout); resolve(String(event.address)); } }
      catch { /* wait for a full structured line */ }
    });
  });
  return { endpoint: `http://${address}/v1/guard`, socket, child,
    /** @param {NodeJS.Signals} [signal] */
    async close(signal = 'SIGINT') { if (child.exitCode === null && child.signalCode === null) {
      const exited = new Promise(resolve => child.once('exit', resolve)); child.kill(signal); await exited; assert.equal(existsSync(socket), false, 'graceful shutdown must unlink its socket');
    } rmSync(directory, { recursive: true, force: true }); } };
}
const envelope = { tool: 'op', connection: 'conn', operation: /** @type {const} */ ('write'), args: { value: 100 }, mode: /** @type {const} */ ('repair'), idempotency_key: 'same-key' };
/** @param {ReturnType<typeof sidecar> extends Promise<infer T> ? T : never} guard @param {object} input */
const invoke = (guard, input) => fetch(guard.endpoint, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify(input) });

test('real loopback and Unix HTTP preserve raw payload bytes and replace forged owned headers', async () => {
  let incoming = ''; let key;
  const upstream = await vendor(async (req, res) => { for await (const chunk of req) incoming += String(chunk); key = req.headers['idempotency-key'];
    res.writeHead(207, { 'x-agindow-status': ['forged', 'forged2'] }).end(incoming); });
  const guard = await sidecar(upstream.url);
  try {
    const raw = '{ "n":1e400,"n":2 }';
    const input = JSON.stringify(envelope).replace('{"value":100}', raw);
    const result = await fetch(guard.endpoint, { method: 'POST', body: input });
    assert.equal(await result.text(), raw); assert.equal(incoming, raw); assert.equal(key, 'same-key');
    assert.equal(result.status, 207); assert.equal(result.headers.get('x-agindow-status'), 'ok');
    const health = await new Promise((resolve, reject) => {
      const req = request({ socketPath: guard.socket, path: '/healthz' }, response => {
        let body = ''; response.on('data', bytes => { body += String(bytes); }); response.once('end', () => resolve(JSON.parse(body)));
      }); req.once('error', reject); req.end();
    });
    assert.equal(health.registered, true);
    assert.equal((await invoke(guard, { ...envelope, authorization: 'inline-secret' })).status, 400);
    assert.equal((await fetch(guard.endpoint, { method: 'POST', headers: { origin: 'https://attacker.invalid' }, body: JSON.stringify(envelope) })).status, 403);
    // fetch replaces Host; use raw HTTP to test a genuine rebinding request.
    const rebinding = await new Promise((resolve, reject) => {
      const req = request(guard.endpoint, { method: 'POST', headers: { host: 'attacker.invalid' } }, response => {
        response.resume(); resolve(response.statusCode);
      }); req.once('error', reject); req.end(JSON.stringify(envelope));
    });
    assert.equal(rebinding, 403);
  } finally { try { await guard.close('SIGTERM'); } finally { await upstream.close(); } }
});
test('real request halts send nothing and publish exact six-field feedback and its audit', async () => {
  let calls = 0; const upstream = await vendor((_req,res) => { calls++; res.end('{}'); });
  const guard = await sidecar(upstream.url, { side: 'request' });
  try {
    for (const mode of ['repair','strict']) {
      const response = await invoke(guard, { ...envelope, mode });
      assert.equal(response.status, 409); assert.equal(response.headers.get('x-agindow-status'), 'blocked');
      const payload = await response.json(); assert.deepEqual(Object.keys(payload).sort(), ['audit','drift_class','drift_id','error','hint','tool']);
      assert.equal(payload.error, mode === 'strict' ? 'drift_halted' : 'drift_blocked');
      const audit = await fetch(new URL(payload.audit, guard.endpoint)); assert.equal(audit.status, 200);
    }
    assert.equal(calls, 0);
  } finally { try { await guard.close(); } finally { await upstream.close(); } }
});
test('internal budget failure preserves the original upstream status/bytes with one bypass header', async () => {
  let calls = 0; const bytes = ' {"value":100}\n';
  const upstream = await vendor((_req,res) => { calls++; res.writeHead(202, { 'content-length': Buffer.byteLength(bytes) }).end(bytes); });
  const guard = await sidecar(upstream.url, { side: 'request', work: 0 });
  try {
    const response = await invoke(guard, { ...envelope, mode: 'strict' });
    assert.equal(response.status, 202); assert.equal(await response.text(), bytes);
    assert.equal(response.headers.get('x-agindow-status'), 'bypassed'); assert.equal(calls, 1);
  } finally { try { await guard.close(); } finally { await upstream.close(); } }
});
test('real response halts retain the completed bytes and SDK outcome without redispatch', async () => {
  let calls = 0; const bytes = ' {"value":100}\n';
  const upstream = await vendor((_req,res) => { calls++; res.writeHead(201, { 'content-length': Buffer.byteLength(bytes) }).end(bytes); });
  const guard = await sidecar(upstream.url, { side: 'response' });
  try {
    try {
      await guardRemote(() => { throw Error('must not replay'); }, envelope, { endpoint: guard.endpoint });
      assert.fail('expected response halt');
    } catch (error) {
      assert.ok(error instanceof AgindowDriftError); assert.match(error.hint, /do not blindly replay/);
      assert.ok(error.outcome instanceof Response); assert.equal(error.outcome.status, 201);
      assert.equal(await error.outcome.text(), bytes);
    }
    assert.equal(calls, 1);
  } finally { try { await guard.close(); } finally { await upstream.close(); } }
});
test('killed real proxy fails open once and preserves one write through the same upstream key', async () => {
  let attempts = 0; let writes = 0;
  /** @type {(() => void) | undefined} */ let started;
  const received = new Promise(resolve => { started = () => resolve(undefined); });
  const upstream = await vendor((req,res) => {
    attempts++; assert.equal(req.headers['idempotency-key'], 'same-key');
    if (writes === 0) { writes++; started?.(); /* original response intentionally pending */ }
    else res.writeHead(201).end('{"deduplicated":true}');
  });
  const guard = await sidecar(upstream.url);
  /** @type {string[]} */ const events = [];
  try {
    let fallback = 0;
    const pending = guardRemote(input => { fallback++; return fetch(upstream.url, { method: 'POST', headers: { 'idempotency-key': input.idempotency_key ?? '' } }); }, envelope,
      { endpoint: guard.endpoint, onEvent: event => events.push(event.type) });
    await received; guard.child.kill('SIGKILL');
    const result = await pending; assert.ok(result instanceof Response); assert.equal(result.status, 201);
    assert.equal(await result.text(), '{"deduplicated":true}');
    assert.equal(writes, 1); assert.equal(attempts, 2); assert.equal(fallback, 1); assert.deepEqual(events, ['bypass']);
  } finally { try { await guard.close(); } finally { await upstream.close(); } }
});
