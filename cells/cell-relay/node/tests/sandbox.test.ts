import { expect, it } from 'vitest';
import { evaluateSandbox } from '../src/sandbox.js';
import type { SandboxContract } from '../src/sandbox.js';
import { generateGuard, instrument } from '../cli/codegen.js';
import { transformSync } from 'esbuild';
import { runInNewContext } from 'node:vm';
const contract: SandboxContract = { version: 1, id: 'fixture', file: 'stripe/actions/create.ts', tool: 'stripe.create_customer',
  method: 'post', endpoint: '/customers', operation: 'write', mode: 'repair',
  request: { fields: { payment_method: { type: 'string', required: true } }, renames: [{ from: 'source', to: 'payment_method', identity: 'payment', confidence: 0.99 }] },
  response: { fields: { status: { type: 'string', enum: ['active'] } } } };
const source = `import { createAction } from 'nango';
export default createAction({ exec: async (nango, input) => {
  const res = await nango.post({ endpoint: '/customers', data: input });
  return res.data;
} });`;
it('the offline sandbox gate repairs only confirmed same-type sibling renames and never mutates recordings', () => {
  const input = { source: 'tok', untouched: 9007199254740992 };
  const result = evaluateSandbox(contract, input, 'request');
  expect(result).toMatchObject({ status: 'repaired', byte_identical: false, value: { payment_method: 'tok', untouched: 9007199254740992 } });
  expect(input).toEqual({ source: 'tok', untouched: 9007199254740992 });
  expect(evaluateSandbox({ ...contract, mode: 'observe' }, input, 'request').value).toBe(input);
  expect(evaluateSandbox(contract, { source: 123 }, 'request')).toMatchObject({ status: 'blocked', driftClass: 'semantic' });
  expect(evaluateSandbox(contract, { status: 'new' }, 'response')).toMatchObject({ status: 'blocked', driftClass: 'semantic' });
});
it('generates one self-contained typed block, installs it idempotently and preserves provider-call order', async () => {
  const generated = generateGuard(contract);
  expect(generated).not.toMatch(/\b(?:import|require|fetch|process|eval|WebAssembly)\b/);
  const first = instrument(source, contract); expect(first).toContain(generated);
  expect(instrument(first, contract)).toBe(first);
  const code = transformSync(first, { loader: 'ts', format: 'cjs' }).code;
  const module: { exports: { default?: { exec: (nango: unknown, input: unknown) => Promise<unknown> } } } = { exports: {} };
  runInNewContext(code, { module, exports: module.exports, require: () => ({ createAction: (value: unknown) => value }) });
  let calls = 0;
  const nango = { ActionError: class extends Error { constructor(readonly payload: unknown) { super('ActionError'); } },
    post: async (request: { data: unknown }) => { calls++; expect(request.data).toEqual({ payment_method: 'tok' }); return { data: { status: 'new' } }; } };
  await expect(module.exports.default?.exec(nango, { source: 'tok' })).rejects.toMatchObject({ payload: { error: 'drift_blocked', drift_class: 'semantic' } });
  expect(calls).toBe(1);
});
it('refuses unsupported call sites and bypasses unavailable bodies without fabricating shape evidence', () => {
  expect(() => instrument('const other = 1;', contract)).toThrow();
  expect(evaluateSandbox(contract, null, 'request').status).toBe('bypassed');
  expect(evaluateSandbox(contract, {}, 'request', true).status).toBe('bypassed');
});
it('refuses to refresh an existing block under a different method or endpoint binding', () => {
  const first = instrument(source, contract);
  expect(() => instrument(first, { ...contract, endpoint: '/other' })).toThrow();
  expect(() => instrument(first, { ...contract, method: 'get' })).toThrow();
});
