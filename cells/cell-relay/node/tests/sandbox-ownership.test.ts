import { expect, it } from 'vitest';
import { transformSync } from 'esbuild';
import { runInNewContext } from 'node:vm';
import { evaluateSandbox } from '../src/sandbox.js';
import type { SandboxContract, SandboxFields } from '../src/sandbox.js';
import { instrument } from '../cli/codegen.js';

function contract(request: SandboxFields): SandboxContract {
  return { version: 1, id: 'ownership', file: 'stripe/actions/create.ts', tool: 'stripe.create',
    method: 'post', endpoint: '/customers', mode: 'repair', operation: 'write', request };
}
function rename(from: string, to: string): NonNullable<SandboxFields['renames']> {
  return [{ from, to, identity: 'payment', confidence: 0.99 }];
}

it.each(['toString', 'valueOf', 'hasOwnProperty'])('ignores absent inherited rename source %s', name => {
  const input = { payment_method: 'tok' };
  const result = evaluateSandbox(contract({ fields: { payment_method: { type: 'string', required: true } },
    renames: rename(name, 'payment_method') }), input, 'request');
  expect(result.status).toBe('ok');
  expect(result.value).toBe(input);
});

it.each(['toString', 'valueOf', 'hasOwnProperty'])('repairs into absent inherited rename target %s', name => {
  const input = { source: 'tok' };
  const result = evaluateSandbox(contract({ fields: { [name]: { type: 'string', required: true } },
    renames: rename('source', name) }), input, 'request');
  expect(result).toMatchObject({ status: 'repaired', value: { [name]: 'tok' } });
  expect(input).toEqual({ source: 'tok' });
});

it.each(['request', 'response'] as const)('ignores absent optional inherited-name fields on %s', side => {
  const schema: SandboxFields = { fields: { toString: { type: 'string' as const } } };
  const input = {};
  const selected = { ...contract(schema), response: schema };
  expect(evaluateSandbox(selected, input, side)).toMatchObject({ status: 'ok', value: input });
});

it('classifies an absent required inherited-name field as shape drift', () => {
  expect(evaluateSandbox(contract({ fields: { toString: { type: 'string' as const, required: true } } }), {}, 'request'))
    .toMatchObject({ status: 'degraded', driftClass: 'shape', changes: [{ path: 'toString', kind: 'required_added' }] });
});

it('validates actual own inherited-name fields', () => {
  const selected = contract({ fields: { toString: { type: 'string' as const } } });
  expect(evaluateSandbox(selected, { toString: 'tok' }, 'request').status).toBe('ok');
  expect(evaluateSandbox(selected, { toString: 123 }, 'request')).toMatchObject({ status: 'blocked', driftClass: 'semantic' });
});

it('renames an actual own inherited-name source', () => {
  expect(evaluateSandbox(contract({ fields: { payment_method: { type: 'string' } },
    renames: rename('toString', 'payment_method') }), { toString: 'tok' }, 'request'))
    .toMatchObject({ status: 'repaired', value: { payment_method: 'tok' } });
});

it('does not use an inherited schema target rule', () => {
  const fields: SandboxFields['fields'] = {};
  Object.setPrototypeOf(fields, { payment_method: { type: 'string' } });
  expect(evaluateSandbox(contract({ fields, renames: rename('source', 'payment_method') }), { source: 'tok' }, 'request'))
    .toMatchObject({ status: 'blocked', driftClass: 'semantic' });
});

it('dispatches a generated wrapper once for a clean request with an absent inherited-name source', async () => {
  const selected = contract({ fields: { payment_method: { type: 'string', required: true } },
    renames: rename('toString', 'payment_method') });
  const source = `import { createAction } from 'nango';
export default createAction({ exec: async (nango, input) => {
  const res = await nango.post({ endpoint: '/customers', data: input });
  return res.data;
} });`;
  const code = transformSync(instrument(source, selected), { loader: 'ts', format: 'cjs' }).code;
  const module: { exports: { default?: { exec: (nango: unknown, input: unknown) => Promise<unknown> } } } = { exports: {} };
  runInNewContext(code, { module, exports: module.exports, require: () => ({ createAction: (value: unknown) => value }) });
  const input = { payment_method: 'tok' }; const output = { id: 'customer' }; let calls = 0;
  const nango = { ActionError: Error, post: async (request: { data: unknown }) => {
    calls++; expect(request.data).toBe(input); return { data: output };
  } };
  expect(module.exports.default).toBeDefined();
  await expect(module.exports.default?.exec(nango, input)).resolves.toBe(output);
  expect(calls).toBe(1);
});
