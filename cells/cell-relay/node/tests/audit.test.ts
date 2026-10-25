import { expect, it } from 'vitest';
import { createAgindow } from '../src/index.js';
import { bindGuest } from '../src/wasm.js';
const pass = JSON.stringify({ halt: false, bypassed: false, class: null, findings: 0, detectors: 0, malformed: false });
it('keeps failed heads and never drains concurrently or from a retired guest', async () => {
  let acknowledgements = 0; let peeks = 0;
  const fixture = { id: 'aud_fixture', ts: '2026-10-20T00:00:00Z', tool: 'op', operation: 'read', mode: 'observe', drift: null,
    decision: 'ok', patch: null, proof: { contract_version: 'unavailable', verified_against: 'cache', write_integrity: false } };
  const guest = { active: () => false, enable_audit() {}, audit_peek() { peeks++; return acknowledgements ? 'null' : JSON.stringify(fixture); },
    audit_ack() { acknowledgements++; return true; }, audit_stats: () => JSON.stringify({ pending: acknowledgements ? 0 : 1, dropped: 0 }),
    begin() { return { audit_context() {}, request: () => pass, finish: () => pass, free() {} }; } };
  const engine = bindGuest(guest, { audit: { tenant: 'tenant' } });
  const sdk = createAgindow({ audit: { tenant: 'tenant' } }, async () => engine); await sdk.ready;
  expect(await sdk.drainAudit(() => { throw Error('offline'); })).toMatchObject({ delivered: 0, pending: 1 });
  expect(acknowledgements).toBe(0);
  let release: (() => void) | undefined;
  const pending = sdk.drainAudit(() => new Promise<void>(resolve => { release = resolve; }));
  await Promise.resolve();
  expect(await sdk.drainAudit(() => { throw Error('duplicate sink entry'); })).toMatchObject({ delivered: 0 });
  release?.(); expect(await pending).toMatchObject({ delivered: 1, pending: 0, available: true });
  expect(acknowledgements).toBe(1); expect(peeks).toBeGreaterThan(0);
});
it('does not invoke sinks absent publication configuration', async () => {
  const sdk = createAgindow({}, async () => ({ begin() { throw Error('unready'); } })); await sdk.ready;
  let calls = 0; expect(await sdk.drainAudit(() => { calls++; })).toMatchObject({ available: false }); expect(calls).toBe(0);
});
it('retires a guest whose drain traps and never re-enters it from later calls', async () => {
  let entries = 0; let upstream = 0;
  const guest = { active: () => false, enable_audit() {}, audit_peek() { entries++; throw new WebAssembly.RuntimeError('unreachable'); },
    audit_ack: () => false, audit_stats: () => '{}', begin() { entries++; throw Error('must not re-enter'); } };
  const engine = bindGuest(guest, { audit: { tenant: 'tenant' } });
  const sdk = createAgindow({ audit: { tenant: 'tenant' } }, async () => engine); await sdk.ready;
  expect(await sdk.drainAudit(() => { throw Error('no sink'); })).toMatchObject({ available: false });
  const result = {}; expect(sdk.guard(() => { upstream++; return result; })()).toBe(result);
  expect(await sdk.drainAudit(() => {})).toMatchObject({ available: false });
  expect(entries).toBe(1); expect(upstream).toBe(1);
});
it('does not acknowledge malformed artifacts or count a refused acknowledgement as delivered', async () => {
  for (const malformed of [true, false]) {
    let acknowledgements = 0; let delivered = 0;
    const artifact = { id: 'aud_fixture', ts: '2026-10-20T00:00:00Z', tool: 'op', operation: 'read', mode: 'observe', drift: null,
      decision: 'ok', patch: null, proof: { contract_version: 'unavailable', verified_against: 'cache', write_integrity: false } };
    const guest = { active: () => false, enable_audit() {}, audit_peek: () => JSON.stringify(malformed ? { id: 'invalid' } : artifact),
      audit_ack() { acknowledgements++; return false; }, audit_stats: () => '{"pending":1,"dropped":0}',
      begin() { return { request: () => pass, finish: () => pass, free() {} }; } };
    const engine = bindGuest(guest, { audit: { tenant: 'tenant' } });
    expect(await engine.drainAudit?.(() => { delivered++; })).toMatchObject({ delivered: 0 });
    expect(acknowledgements).toBe(malformed ? 0 : 1); expect(delivered).toBe(malformed ? 0 : 1);
  }
});
