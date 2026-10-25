import fc from 'fast-check';
import { describe, it, expect } from 'vitest';
import { createAgindow } from '../src/index.js';

describe('Node distribution', () => {
  it('preserves sync identity, every argument and this', async () => {
    const sdk = createAgindow();
    await sdk.ready;
    const original = {};
    const fn = sdk.guard(function(this: { n: number }, a: number, b: object) {
      expect(this.n).toBe(a); return b;
    });
    expect(fn.call({ n: 4 }, 4, original)).toBe(original);
  });
  it('wraps functions and framework-neutral tool metadata with exact types', async () => {
    const sdk = createAgindow(); await sdk.ready;
    const schema = {};
    const tools = sdk.wrap({ plain: (n: number) => n + 1,
      tool: { description: 'hi', schema, execute: async (s: string) => s },
    }, { mode: 'observe' });
    expect(tools.plain(1)).toBe(2);
    expect(await tools.tool.execute('yes')).toBe('yes');
    expect(tools.tool.schema).toBe(schema);
    expect(tools.tool.description).toBe('hi');
  });
  it('preserves errors from sync and ordinary promise-returning functions', async () => {
    const sdk = createAgindow(); await sdk.ready;
    const error = {};
    expect(() => sdk.guard(() => { throw error; })()).toThrow();
    try { sdk.guard(() => { throw error; })(); } catch (caught) { expect(caught).toBe(error); }
    await expect(sdk.guard(() => Promise.reject(error))()).rejects.toBe(error);
  });
  it('bypasses once while loading and after load failures', async () => {
    const events: string[] = [];
    const sdk = createAgindow({ onEvent: e => events.push(e.type) }, async () => { throw Error('load'); });
    const original = {};
    expect(sdk.guard(() => original)()).toBe(original);
    await sdk.ready;
    expect(sdk.guard(() => original)()).toBe(original);
    expect(events).toEqual(['bypass', 'bypass']);
  });
});
it('leaves non-executable entries untouched', () => {
  const sdk = createAgindow();
  const date = new Date(0);
  const metadata = { note: 'not a tool' };
  const wrapped = sdk.wrap({ date, metadata });
  expect(wrapped.date).toBe(date);
  expect(wrapped.metadata).toBe(metadata);
});
it('leaves class tools with private state callable and warns that an adapter is required', () => {
  class Counter {
    #count = 0;
    execute = function(this: Counter): number { return ++this.#count; };
  }
  const counter = new Counter();
  const original = console.warn;
  const warnings: unknown[][] = [];
  try {
    console.warn = (...args: unknown[]) => { warnings.push(args); };
    const wrapped = createAgindow().wrap({ counter });
    expect(wrapped.counter).toBe(counter);
    expect(wrapped.counter.execute()).toBe(1);
    expect(warnings).toHaveLength(1);
  } finally { console.warn = original; }
});

it('property-preserves values, receivers and every argument without cloning sync or async results', async () => {
  const sdk = createAgindow(); await sdk.ready;
  await fc.assert(fc.asyncProperty(fc.jsonValue(), fc.array(fc.jsonValue()), async (payload, tail) => {
    const receiver = { payload };
    const args = [payload, ...tail];
    const original = { payload, symbol: Symbol('opaque') };
    const before = JSON.stringify(payload);
    const fn = function(this: typeof receiver, ...received: unknown[]) {
      expect(this).toBe(receiver); expect(received).toEqual(args);
      received.forEach((value, index) => expect(value).toBe(args[index]));
      return original;
    };
    expect(sdk.guard(fn).apply(receiver, args)).toBe(original);
    const pending = sdk.guard(function(this: typeof receiver, ...received: unknown[]) {
      return Promise.resolve(fn.apply(this, received));
    }).apply(receiver, args);
    expect(pending).toBeInstanceOf(Promise); expect(await pending).toBe(original);
    expect(JSON.stringify(payload)).toBe(before);
  }), { numRuns: 100 });
});
