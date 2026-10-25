import { expect, it, vi } from 'vitest';
import fc from 'fast-check';
import { createAgindow, AgindowSetupError } from '../src/index.js';
import type { Engine, Verdict } from '../src/types.js';
const pass: Verdict = { halt: false, bypassed: false, class: null, findings: 0, detectors: 0, malformed: false };
function recording(tools: string[]): Engine { return { begin(_v, _m, _o, _f, tool) {
  tools.push(tool); return { request: pass, finish: () => pass, close() {} };
} }; }
it('property-preserves all own metadata, schemas, symbols and callable types on maps and arrays', async () => {
  const sdk = createAgindow({}, async () => recording([])); await sdk.ready;
  fc.assert(fc.property(fc.string(), fc.boolean(), fc.dictionary(fc.string(), fc.jsonValue()), (name, flag, schema) => {
    const symbol = Symbol('meta');
    for (const implementation of ['execute', 'func'] as const) {
      const tool = { name, id: name, description: name, schema, inputSchema: schema, zodSchema: schema,
        returnsSingleValue: flag, [symbol]: schema, [implementation]: () => schema };
      const wrapped = sdk.toolAdapter([tool])[0];
      expect(wrapped).toBeDefined();
      if (wrapped === undefined) throw Error('lost tool');
      expect(Reflect.ownKeys(wrapped)).toEqual(Reflect.ownKeys(tool));
      for (const key of Reflect.ownKeys(tool).filter(key => key !== implementation)) {
        expect(Reflect.get(wrapped, key)).toBe(Reflect.get(tool, key));
      }
      expect(Reflect.apply(Reflect.get(wrapped, implementation) as () => unknown, wrapped, [])).toBe(schema);
      const map = sdk.toolAdapter({ run: () => schema }); expect(map.run()).toBe(schema);
    }
  }), { numRuns: 100 });
});
it('contains broken entries and never invokes metadata getters during installation', () => {
  const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
  const getter = vi.fn(() => { throw Error('getter'); });
  const broken = Object.defineProperty({}, 'execute', { get: getter });
  const sdk = createAgindow(); const wrapped = sdk.toolAdapter({ broken, ok: () => 4 });
  expect(wrapped.broken).toBe(broken); expect(wrapped.ok()).toBe(4); expect(getter).not.toHaveBeenCalled();
  expect(warn).toHaveBeenCalled(); warn.mockRestore();
});
it('tool executor state stays on the original tool and explicit receivers are preserved', async () => {
  const sdk = createAgindow({}, async () => recording([])); await sdk.ready;
  const original = { count: 0, execute(this: { count: number }) { this.count++; return this.count; } };
  const wrapped = sdk.toolAdapter({ counter: original }).counter;
  expect(wrapped.execute()).toBe(1); expect(original.count).toBe(1);
  const receiver = { count: 10 }; expect(wrapped.execute.call(receiver)).toBe(11); expect(receiver.count).toBe(11);
});
it('wraps exactly the HTTP allow-list across own and prototype methods, preserves private state', async () => {
  const calls: string[] = []; const sdk = createAgindow({}, async () => recording(calls)); await sdk.ready;
  class Client {
    #value = 4;
    get() { return this.#value; } post = () => this.#value;
    put() { return this.#value; } patch() { return this.#value; }
    delete() { return this.#value; } request() { return this.#value; }
    fetch() { return this.#value; } proxy() { return this.#value; }
    auth() { return this.#value; } connect() {} disconnect() {} batchSave() {} getCheckpoint() {} saveCheckpoint() {}
  }
  const client = new Client(); const wrapped = sdk.clientAdapter(client, { tool: 'vendor.call' });
  expect(Object.keys(wrapped)).toEqual(Object.keys(client));
  for (const name of ['get', 'post', 'put', 'patch', 'delete', 'request', 'fetch', 'proxy'] as const) expect(wrapped[name]()).toBe(4);
  expect(calls).toEqual(Array(8).fill('vendor.call'));
  for (const name of ['auth', 'connect', 'disconnect', 'batchSave', 'getCheckpoint', 'saveCheckpoint'] as const) {
    Reflect.apply(wrapped[name], wrapped, []);
  }
  expect(wrapped.auth()).toBe(4);
  expect(calls).toHaveLength(8);
  const plain = { get: () => 4, batchSave: () => 1, getCheckpoint: () => 2, saveCheckpoint: () => 3 };
  const plainWrapped = sdk.clientAdapter(plain);
  expect(plainWrapped.batchSave).toBe(plain.batchSave);
  expect(plainWrapped.getCheckpoint).toBe(plain.getCheckpoint);
  expect(plainWrapped.saveCheckpoint).toBe(plain.saveCheckpoint);
});
it('resolves a client identity per call and auto-detection is equivalent to explicit adapters', async () => {
  const tools: string[] = []; const sdk = createAgindow({}, async () => recording(tools)); await sdk.ready;
  const client = { get: (req: { endpoint: string }) => req };
  const wrapped = sdk.clientAdapter(client, { tool: (req: { endpoint: string }) => `nango/${req.endpoint}` });
  const req = { endpoint: 'customers' }; expect(wrapped.get(req)).toBe(req);
  expect(tools).toEqual(['nango/customers']);
  expect(sdk.wrap(client).get(req)).toBe(req);
  const array = [{ execute: () => req }]; expect(sdk.wrap(array)[0]?.execute()).toBe(req);
});
it('rejects ambiguous detection synchronously with explicit alternatives', () => {
  const sdk = createAgindow(); const called = vi.fn();
  expect(() => sdk.wrap({ get: called, search: called })).toThrow(AgindowSetupError);
  expect(() => sdk.wrap({ get: called, search: called })).toThrow(/toolAdapter.*clientAdapter/);
  expect(called).not.toHaveBeenCalled();
});
it('leaves a class tool with an immutable invoke slot intact instead of violating Proxy invariants', async () => {
  const warning = vi.spyOn(console, 'warn').mockImplementation(() => {});
  try {
    const calls: string[] = []; const sdk = createAgindow({}, async () => recording(calls)); await sdk.ready;
    class Tool {
      name = 'frozen'; schema = {};
      invoke() { return 42; }
      constructor() { Object.defineProperty(this, 'invoke', { value: this.invoke, writable: false, configurable: false }); }
    }
    const tool = new Tool(); const wrapped = sdk.toolAdapter({ tool });
    expect(wrapped.tool).toBe(tool); expect(wrapped.tool.invoke()).toBe(42);
    expect(calls).toEqual([]); expect(warning).toHaveBeenCalled();
  } finally { warning.mockRestore(); }
});
it('contains installation when an immutable branded client helper cannot preserve its receiver', async () => {
  const warning = vi.spyOn(console, 'warn').mockImplementation(() => {});
  try {
    const calls: string[] = []; const sdk = createAgindow({}, async () => recording(calls)); await sdk.ready;
    class Client {
      #value = 42;
      get() { return this.#value; }
      auth() { return this.#value; }
      constructor() { Object.defineProperty(this, 'auth', { value: this.auth, writable: false, configurable: false }); }
    }
    const client = new Client(); const wrapped = sdk.clientAdapter(client);
    expect(wrapped).toBe(client); expect(wrapped.auth()).toBe(42); expect(wrapped.get()).toBe(42);
    expect(calls).toEqual([]); expect(warning).toHaveBeenCalled();
  } finally { warning.mockRestore(); }
});

it('guards Mastra and AI SDK execute shapes while retaining context and schema identity', async () => {
  const selected: string[] = []; const sdk = createAgindow({}, async () => recording(selected)); await sdk.ready;
  const schema = { parse: (value: unknown) => value }; const context = { runId: 'fixture' };
  const mastra = { id: 'mastra.lookup', description: 'lookup', inputSchema: schema,
    execute: (input: { context: object }) => input.context };
  const ai = { description: 'AI SDK', parameters: schema,
    execute: (input: object, options: object) => ({ input, options }) };
  const wrapped = sdk.toolAdapter({ mastra, ai });
  expect(wrapped.mastra.execute({ context })).toBe(context);
  expect(wrapped.ai.execute(context, context)).toEqual({ input: context, options: context });
  expect(wrapped.mastra.inputSchema).toBe(schema); expect(wrapped.ai.parameters).toBe(schema);
  expect(Object.keys(wrapped.mastra)).toEqual(Object.keys(mastra));
  expect(Object.keys(wrapped.ai)).toEqual(Object.keys(ai));
  expect(selected).toEqual(['mastra.lookup', 'ai']);
});
it('contains target reflection faults during explicit adapter installation and detection', () => {
  const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
  try {
    const target = new Proxy({ get: () => 42 }, { ownKeys() { throw Error('reflection'); } });
    const sdk = createAgindow();
    for (const install of [sdk.toolAdapter, sdk.clientAdapter, sdk.wrap]) {
      expect(install(target)).toBe(target); expect(target.get()).toBe(42);
    }
    expect(warn).toHaveBeenCalledTimes(3);
  } finally { warn.mockRestore(); }
});
it('auto-detects a fetch HTTP surface and matches explicit raw-response inspection', async () => {
  const captured: unknown[] = [];
  const sdk = createAgindow({}, async () => ({ begin() { return { request: pass,
    finish(value) { captured.push(value); return pass; }, close() {} }; } })); await sdk.ready;
  async function fetch() { return new Response('{}', { headers: { 'content-type': 'application/json', 'content-length': '2' } }); }
  expect(await (await sdk.wrap(fetch)()).text()).toBe('{}');
  expect(await (await sdk.httpAdapter(fetch)()).text()).toBe('{}');
  expect(captured).toHaveLength(2);
  for (const value of captured) expect(value).toHaveProperty('bytes');
});
it('contains HTTP adapter installation faults and preserves the original callable', () => {
  const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
  try {
    const target = new Proxy(() => 42, { ownKeys() { throw Error('reflection'); } });
    const sdk = createAgindow();
    expect(sdk.httpAdapter(target)).toBe(target); expect(target()).toBe(42);
    expect(warn).toHaveBeenCalledOnce();
  } finally { warn.mockRestore(); }
});
