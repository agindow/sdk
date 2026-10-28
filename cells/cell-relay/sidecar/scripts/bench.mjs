import { createServer, request, Agent } from 'node:http';
import { spawn } from 'node:child_process';
import { performance } from 'node:perf_hooks';
import { cpus, tmpdir, platform, arch } from 'node:os';
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { join } from 'node:path';
import { summarize } from '../../node/scripts/statistical-report.mjs';
const directory = mkdtempSync(join(tmpdir(), 'agindow-socket-bench-'));
const socket = join(directory, 'guard.sock');
const output = new URL('../../../../target/sidecar-loop/', import.meta.url);
mkdirSync(output, { recursive: true });
const payload = JSON.stringify({ value: 1, padding: 'x'.repeat(838) });
const agent = new Agent({ keepAlive: true, maxSockets: 1 });
let calls = 0;
const vendor = createServer(async (incoming, outgoing) => { for await (const _chunk of incoming) { /* drain */ } calls++;
  outgoing.writeHead(200, { 'content-type': 'application/json', 'content-length': Buffer.byteLength(payload) }).end(payload); });
await new Promise(resolve => vendor.listen(0, '127.0.0.1', () => resolve(undefined)));
const address = vendor.address(); if (address === null || typeof address === 'string') throw Error('missing upstream address');
const upstreamPort = address.port;
const upstream = `http://127.0.0.1:${address.port}`;
const binary = process.env['AGINDOW_SIDECAR_BIN'] ?? fileURLToPath(new URL('../../../../target/release/agindow-sidecar', import.meta.url));
const child = spawn(binary, [], { env: { ...process.env, AGINDOW_BIND: '127.0.0.1:0', AGINDOW_SOCKET: socket,
  AGINDOW_CONNECTIONS: JSON.stringify({ 'c/op': { url: upstream, method: 'POST', operation: 'read' } }) }, stdio: ['ignore','pipe','pipe'] });
/** @param {boolean} guarded */
async function invoke(guarded) {
  const body = guarded ? `{"tool":"op","connection":"c","operation":"read","args":${payload},"mode":"observe"}` : payload;
  return new Promise((resolve, reject) => {
    const req = request(guarded ? { socketPath: socket, path: '/v1/guard', method: 'POST', agent,
      headers: { 'content-type': 'application/json', 'content-length': Buffer.byteLength(body) } } : { hostname: '127.0.0.1', port: upstreamPort, method: 'POST', agent,
      headers: { 'content-length': Buffer.byteLength(body) } }, response => {
      let text = ''; response.on('data', bytes => { text += String(bytes); }); response.once('error', reject);
      response.once('end', () => { if (response.statusCode !== 200 || text !== payload) reject(Error('lost upstream outcome')); else resolve(undefined); });
    }); req.once('error', reject); req.end(body);
  });
}
try {
  await new Promise((resolve, reject) => {
    const deadline = setTimeout(() => reject(Error('sidecar startup timeout')), 30000);
    child.once('error', reject); child.once('exit', () => { clearTimeout(deadline); reject(Error('sidecar exited')); });
    child.stdout.once('data', () => { clearTimeout(deadline); resolve(undefined); });
  });
  for (let i = 0; i < 1000; i++) { await invoke(false); await invoke(true); }
  const raw = { guarded: /** @type {number[]} */ ([]), unguarded: /** @type {number[]} */ ([]), added: /** @type {number[]} */ ([]) };
  for (let i = 0; i < 5000; i++) {
    let guardedUs = 0; let unguardedUs = 0;
    for (const guarded of i % 2 ? [true,false] : [false,true]) {
      const start = performance.now(); await invoke(guarded); const us = (performance.now() - start) * 1000;
      if (guarded) guardedUs = us; else unguardedUs = us;
    }
    raw.guarded.push(guardedUs); raw.unguarded.push(unguardedUs); raw.added.push(guardedUs - unguardedUs);
  }
  if (calls !== 12000) throw Error('wrong upstream counts');
  const summary = Object.fromEntries(Object.entries(raw).map(([key, values]) => [key, summarize(values)]));
  const report = { schema: 'agindow.sidecar-loop/1', samples: 5000, warmups: 1000, raw, summary, calls,
    payloadBytes: Buffer.byteLength(payload), fixture: 'release empty statistical plan, Unix HTTP socket versus direct loopback upstream; keep-alive',
    estimator: 'nearest-rank individual paired added socket RTT; alternating guarded/control',
    environment: { node: process.version, platform: platform(), arch: arch(), cpu: cpus()[0]?.model },
    exclusions: ['cold process startup', 'statistical engine', 'remote network', 'deployment qualification'] };
  writeFileSync(new URL('report.json', output), JSON.stringify(report) + '\n');
  console.log(JSON.stringify(summary));
  if (Number(summary['added']?.p99) >= 1000) throw Error('sidecar added p99 exceeds 1 ms');
} finally {
  agent.destroy(); vendor.closeAllConnections(); await new Promise(resolve => vendor.close(resolve));
  if (child.exitCode === null && child.signalCode === null) { const exited = new Promise(resolve => child.once('exit', resolve)); child.kill('SIGINT'); await exited; }
  rmSync(directory, { recursive: true, force: true });
}
