import { expect, it } from 'vitest';
import { rotationReport } from '../cli/rotation/index.js';
const base = { contract: 'credential_event', version: 1, id: 'e1', ts: '2026-10-22T00:00:00Z', tenant: 't1', connection: 'c1', operation: 'read', status: 401, cause: 'rejected', evidence: 'unknown' };
async function* events(count = 1): AsyncGenerator<unknown> { for (let n = 0; n < count; n++) yield { ...base, id: `e${n}` }; }
const options = { now: '2026-10-22T01:00:00Z', staleMs: 86400000, connectionCap: 10, operationCap: 10 };
it('renders honest readiness with deterministic bounded aggregation', async () => {
  const a = await rotationReport(events(), [], options);
  expect(a.rows[0]).toMatchObject({ verdict: 'unknown', reason: 'no_capability_facts', auth_breadth: 'unknown' });
  const facts = [{ tenant: 't1', connection: 'c1', observed_at: '2026-10-22T00:00:00Z', source: 'host-1', dual_key: 'yes', mint: 'manual', replacement: true }];
  const b = await rotationReport(events(1000), facts, options);
  expect(b.rows[0]?.verdict).toBe('verifiable');
  expect(JSON.stringify(b)).toBe(JSON.stringify(await rotationReport(events(1000), facts, options)));
  expect(JSON.stringify(b)).not.toMatch(/sk_live_|zero-touch|automatic/u);
});
it('does not silently choose stale, disputed or atomic facts', async () => {
  const f = { tenant: 't1', connection: 'c1', observed_at: options.now, source: 'host', dual_key: 'no', mint: 'manual', replacement: true };
  expect((await rotationReport(events(), [f], options)).rows[0]).toMatchObject({ verdict: 'flag_day', warning: 'Atomic invalidation: cutover cannot be verified without downtime.' });
  expect((await rotationReport(events(), [{ ...f, replacement: false }], options)).rows[0]?.verdict).toBe('unsupported');
  expect((await rotationReport(events(), [{ ...f, observed_at: '2020-01-01T00:00:00Z' }], options)).rows[0]?.reason).toBe('stale_facts');
  expect((await rotationReport(events(), [f, { ...f, source: 'other', dual_key: 'yes' }], options)).rows[0]?.reason).toBe('disputed_facts');
});
it('rejects secret-shaped references and malformed input without rendering a partial report', async () => {
  async function* bad(): AsyncGenerator<unknown> { yield base; yield { ...base, connection: 'sk_live_AGINDOWcanary123' }; }
  await expect(rotationReport(bad(), [], options)).rejects.toThrow();
});
it('streams ten times the event cap without accumulating events', async () => {
  expect((await rotationReport(events(100000), [], options)).rows).toHaveLength(1);
});
it('does not turn a quota event into credential failure or infer credential age/scopes', async () => {
 async function* quota(): AsyncGenerator<unknown> { yield {...base,cause:'recovered',evidence:'provider_declared',prior_event:'failure-1',status:200}; yield {...base,id:'quota-1',ts:'2026-10-22T00:01:00Z',cause:'quota',evidence:'provider_declared',status:403}; }
 const result = await rotationReport(quota(),[],options);
 expect(result.rows[0]?.health).toBe('healthy');
 expect(result.rows[0]?.auth_breadth).toBe('unknown');
 expect(result.rows[0]).not.toHaveProperty('age');
 expect(result.rows[0]).not.toHaveProperty('issued_at');
});
it('refuses capacity overflow and never contacts the network', async () => {
 const original = globalThis.fetch;
 globalThis.fetch = (): Promise<Response> => { throw new Error('Network forbidden'); };
 try {
  async function* many(): AsyncGenerator<unknown> { for(let n=0;n<11;n++) yield {...base,connection:`c${n}`}; }
  await expect(rotationReport(many(),[],options)).rejects.toThrow('Invalid rotation report input');
  expect((await rotationReport(events(),[],options)).rows).toHaveLength(1);
 } finally { globalThis.fetch = original; }
});
it('retains facts-only connections without inventing traffic observations', async () => {
 async function* empty(): AsyncGenerator<unknown> { return; }
 const result = await rotationReport(empty(),[{tenant:'t1',connection:'c1',observed_at:options.now,source:'host',dual_key:'yes',mint:'manual',replacement:true}],options);
 expect(result.rows[0]).toMatchObject({verdict:'verifiable',health:'unknown',observed_since:null,last_ok:null,observed_operations:[]});
});
it('does not let a quota-first export hide an older recovery', async () => {
 async function* unordered(): AsyncGenerator<unknown> {
  yield {...base,id:'quota',ts:'2026-10-22T00:01:00Z',cause:'quota',evidence:'provider_declared',status:403};
  yield {...base,id:'recovery',cause:'recovered',evidence:'provider_declared',prior_event:'failure',status:200};
 }
 expect((await rotationReport(unordered(),[],options)).rows[0]?.health).toBe('healthy');
});
it('renders a new rotated binding as degrading until a success is observed', async () => {
 async function* rotated(): AsyncGenerator<unknown> {
  yield {...base,cause:'rotated',evidence:'host_declared',credential_version:'v2',prior_version:'v1',status:200};
 }
 expect((await rotationReport(rotated(),[],options)).rows[0]?.health).toBe('degrading');
});
it('keeps current-version health separate from late outcomes on the superseded credential', async () => {
 async function* overlap(): AsyncGenerator<unknown> {
  yield {...base,id:'old',credential_version:'v1'};
  yield {...base,id:'rotation',ts:'2026-10-22T00:01:00Z',cause:'rotated',evidence:'host_declared',credential_version:'v2',prior_version:'v1',status:200};
  yield {...base,id:'new-failure',ts:'2026-10-22T00:02:00Z',credential_version:'v2'};
  yield {...base,id:'new-recovery',ts:'2026-10-22T00:03:00Z',cause:'recovered',evidence:'provider_declared',credential_version:'v2',prior_event:'new-failure',status:200};
  yield {...base,id:'old-expired',ts:'2026-10-22T00:04:00Z',cause:'expired',evidence:'provider_declared',credential_version:'v1'};
 }
 expect((await rotationReport(overlap(),[],options)).rows[0]).toMatchObject({health:'healthy',last_ok:'2026-10-22T00:03:00.000Z'});
});
it('uses host export order for outcomes sharing the rotation timestamp', async () => {
 async function* simultaneous(): AsyncGenerator<unknown> {
  yield {...base,id:'z-rotation',cause:'rotated',evidence:'host_declared',credential_version:'v2',prior_version:'v1',status:200};
  yield {...base,id:'a-expired',cause:'expired',evidence:'provider_declared',credential_version:'v2'};
 }
 expect((await rotationReport(simultaneous(),[],options)).rows[0]?.health).toBe('failing');
});
it('does not pool failures from versions when the export has no rotation authority', async () => {
 async function* ambiguous(): AsyncGenerator<unknown> {
  yield {...base,id:'one',credential_version:'v1'};
  yield {...base,id:'two',credential_version:'v2'};
  yield {...base,id:'three',credential_version:'v1'};
 }
 expect((await rotationReport(ambiguous(),[],options)).rows[0]?.health).toBe('unknown');
});
