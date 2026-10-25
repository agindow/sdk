import { expect, it, vi } from 'vitest';
import { PipedreamClient } from '@pipedream/sdk';
import { DynamicStructuredTool } from '@langchain/core/tools';
import { ToolMessage } from '@langchain/core/messages';
import { createRequire } from 'node:module';
import { bindGuest } from '../src/wasm.js';
import { createAgindow } from '../src/index.js';
import type { Verdict } from '../src/types.js';
const pass: Verdict = { halt: false, bypassed: false, class: null, findings: 0, detectors: 0, malformed: false };
it('Pipedream first: wraps the actual SDK proxy with an offline transport and no credentials', async () => {
  let requests = 0; const selected: unknown[] = [];
  const sdk = createAgindow({}, async () => ({ begin(value) {
    selected.push(value); return { request: pass, finish: () => pass, close() {} };
  } })); await sdk.ready;
  const fetch = vi.fn(async () => {
    requests++; return new Response('{"ok":true}', { headers: { 'content-type': 'application/json' } });
  });
  vi.stubGlobal('fetch', fetch);
  const pd = new PipedreamClient({ projectId: 'fixture', tokenProvider: { getToken: async () => 'offline-fixture' } });
  const proxy = sdk.clientAdapter(pd.proxy, { tool: 'slack.chat', operation: 'write' });
  try { expect(await proxy.post({ externalUserId: 'fixture', accountId: 'fixture', url: 'https://slack.com/api/chat.postMessage', body: { text: 'hi' } })).toMatchObject({ ok: true }); }
  finally { vi.unstubAllGlobals(); }
  expect(requests).toBe(1); expect(selected).toHaveLength(1);
});
it('LangChain second: preserves ToolCall formatting and intercepts invoke without modifying the instance', async () => {
  let runs = 0; const selected: unknown[] = [];
  const schema = { type: 'object', properties: { query: { type: 'string' } }, required: ['query'] };
  const tool = new DynamicStructuredTool({ name: 'search', description: 'search records', schema,
    func: async () => { runs++; return 'found'; } });
  const originalInvoke = tool.invoke;
  const sdk = createAgindow({}, async () => ({ begin(value) {
    selected.push(value); return { request: pass, finish: () => pass, close() {} };
  } })); await sdk.ready;
  const wrapped = sdk.toolAdapter([tool])[0]; if (wrapped === undefined) throw Error('missing tool');
  expect(Object.keys(wrapped)).toEqual(Object.keys(tool)); expect(wrapped.schema).toBe(tool.schema);
  expect(wrapped.description).toBe(tool.description); expect(wrapped.func).toBe(tool.func);
  const result = await wrapped.invoke({ type: 'tool_call', id: 'call_1', name: 'search', args: { query: 'hi' } });
  expect(result).toMatchObject({ content: 'found', tool_call_id: 'call_1', name: 'search' });
  expect(runs).toBe(1); expect(selected).toEqual([{ query: 'hi' }]); expect(tool.invoke).toBe(originalInvoke);
});
it('LangChain request repairs retain the ToolCall envelope and configuration', async () => {
  const schema = { type: 'object' as const, properties: { renamed: { type: 'number' as const } }, required: ['renamed'] };
  const tool = new DynamicStructuredTool({ name: 'op', description: 'fixture', schema,
    func: async input => { expect(input).toEqual({ renamed: 7 }); return '7'; } });
  const request: Verdict = { ...pass, findings: 1, class: 'shape', confidence: .99,
    repair: { verified: true, value: { renamed: 7 }, changes: [{ path: '/old', kind: 'field_renamed', to: '/renamed' }] } };
  const sdk = createAgindow({ mode: 'repair' }, async () => ({ begin: () => ({ request, finish: () => pass, close() {} }) }));
  await sdk.ready;
  const wrapped = sdk.toolAdapter([tool])[0]; if (wrapped === undefined) throw Error('missing tool');
  const input = { type: 'tool_call' as const, id: 'call_repaired', name: 'op', args: { old: 7 } };
  const output = await wrapped.invoke(input);
  expect(output).toMatchObject({ tool_call_id: 'call_repaired', name: 'op', content: '7' });
  expect(input.args).toEqual({ old: 7 });
});
it('LangChain formatted JSON reaches real WASM response classification and retains the completed ToolMessage', async () => {
  const tool = new DynamicStructuredTool({ name: 'op', description: 'fixture', schema: { type: 'object', properties: {} },
    func: async () => ({ value: 100 }) });
  const sdk = createAgindow({ mode: 'strict', selection: { tenant: 't', connection: 'c', vendor: 'v', operation: 'op', segment: 's',
    fields: [{ side: 'response', path: '/value', baseline: Array<string>(128).fill('{"value":1}'),
      live: Array<string>(128).fill('{"value":100}'), enumeration: true }] } }, async options => {
    // Source tests use the real packaged guest with the source host boundary;
    // filename substitution belongs to the ESM/CJS distribution build.
    const binding = createRequire(import.meta.url)('../dist/guest/agw_node_guest.js') as {
      Guest: new (selection: string, now: number) => Parameters<typeof bindGuest>[0];
    };
    return bindGuest(new binding.Guest(JSON.stringify(options.selection), performance.now()), options);
  });
  await sdk.ready;
  const wrapped = sdk.toolAdapter([tool], { operation: 'write' })[0]; if (wrapped === undefined) throw Error('missing tool');
  await expect(wrapped.invoke({ type: 'tool_call', id: 'call_completed', name: 'op', args: {} })).rejects.toMatchObject({
    name: 'AgindowDriftError', phase: 'response', driftClass: 'semantic',
    outcome: { tool_call_id: 'call_completed', name: 'op', content: '{"value":100}' },
  });
});

it('keeps unsupported framework response repairs from replacing a completed ToolMessage', async () => {
  const completed = new ToolMessage({ content: 'original', tool_call_id: 'call_original', name: 'op' });
  const tool = new DynamicStructuredTool({ name: 'op', description: 'fixture', schema: { type: 'object', properties: {} },
    func: async () => completed });
  const response: Verdict = { ...pass, class: 'shape', confidence: .99,
    repair: { verified: true, value: { replacement: true }, changes: [] } };
  const sdk = createAgindow({ mode: 'repair' }, async () => ({ begin: () => ({ request: pass, finish: () => response, close() {} }) }));
  await sdk.ready;
  const wrapped = sdk.toolAdapter([tool])[0]; if (wrapped === undefined) throw Error('missing tool');
  expect(await wrapped.invoke({ type: 'tool_call', id: 'call_original', name: 'op', args: {} })).toBe(completed);
  expect(sdk.recent().filter(event => event.type === 'bypass')).toHaveLength(1);
  expect(sdk.recent().filter(event => event.type === 'repair')).toHaveLength(0);
});
