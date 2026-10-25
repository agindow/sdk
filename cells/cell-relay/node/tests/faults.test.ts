import { expect, it } from 'vitest';
import { createAgindow } from '../src/index.js';
import type { Engine, EngineCall, Verdict } from '../src/types.js';
const pass: Verdict = { halt: false, bypassed: false, class: null, findings: 0, detectors: 0, malformed: false };

it('contains evaluation, readiness and event callback faults without retrying upstream', async () => {
  for (const engine of [undefined, { begin() { throw Error('evaluate'); } }]) {
    let bypasses = 0;
    const sdk = createAgindow({ onEvent: event => { if (event.type === 'bypass') bypasses++; throw Error('event'); } },
      async () => { if (engine === undefined) throw Error('not ready'); return engine; });
    await sdk.ready;
    let calls = 0;
    const original = {};
    expect(sdk.guard(() => { calls++; return original; })()).toBe(original);
    expect(calls).toBe(1);
    expect(bypasses).toBe(1);
  }
});
it('contains request result access, response and disposal faults with one bypass', async () => {
  for (const phase of ['request', 'response', 'close']) {
    const engine: Engine = { begin(): EngineCall {
      return { get request() { if (phase === 'request') throw Error('result'); return pass; },
        finish() { if (phase === 'response') throw Error('finish'); return pass; },
        close() { if (phase === 'close') throw Error('close'); },
      };
    } };
    const events: string[] = [];
    const sdk = createAgindow({ onEvent: e => events.push(e.type) }, async () => engine);
    await sdk.ready;
    const original = {};
    expect(await sdk.guard(() => Promise.resolve(original))()).toBe(original);
    expect(events).toEqual(['error', 'bypass']);
  }
});
it('freezes configuration through promise settlement and applies reload to the next call', async () => {
  const modes: string[] = [];
  const engine: Engine = { begin(_value, mode) {
    modes.push(mode);
    return { request: pass, finish() { modes.push(mode); return pass; }, close() {} };
  } };
  const sdk = createAgindow({}, async () => engine); await sdk.ready;
  let release: ((value: object) => void) | undefined;
  const fn = sdk.guard(() => new Promise<object>(resolve => { release = resolve; }));
  const pending = fn(); sdk.configure({ mode: 'strict' }); release?.({}); await pending;
  const next = fn(); release?.({}); await next;
  expect(modes).toEqual(['observe', 'observe', 'strict', 'strict']);
});
it('does not change configuration after a rejected reload', async () => {
  const sdk = createAgindow({ mode: 'observe' }); await sdk.ready;
  expect(() => sdk.configure({ mode: 'invalid' as 'observe' })).toThrow('mode');
  const value = {};
  expect(sdk.guard(() => value)()).toBe(value);
});
it('keeps a framework tool canonical name for engine scope and preserves symbols', async () => {
  const selected: string[] = [];
  const engine: Engine = { begin(_value, _mode, _operation, _failClosed, tool) {
    selected.push(tool);
    return { request: pass, finish: () => pass, close() {} };
  } };
  const sdk = createAgindow({}, async () => engine); await sdk.ready;
  const symbol = Symbol('metadata');
  const tools = sdk.wrap({ alias: { name: 'vendor.lookup', [symbol]: 42, execute: () => 1 } });
  expect(tools.alias.execute()).toBe(1);
  expect(tools.alias[symbol]).toBe(42);
  expect(selected).toEqual(['vendor.lookup']);
});
it('enforces observe at the host choke point even if the engine returns a halt', async () => {
  const halt = { ...pass, halt: true, findings: 1, class: 'semantic' as const };
  const sdk = createAgindow({ mode: 'observe' }, async () => ({ begin() {
    return { request: halt, finish: () => halt, close() {} };
  } }));
  await sdk.ready;
  const original = {};
  expect(sdk.guard(() => original, { operation: 'write' })()).toBe(original);
});
it('warns locally when write protection is explicitly disabled and contains warning failures', async () => {
  const original = console.warn;
  const warnings: unknown[][] = [];
  try {
    console.warn = (...args: unknown[]) => { warnings.push(args); };
    const sdk = createAgindow({ failClosed: false }); await sdk.ready;
    expect(warnings).toHaveLength(1);
    expect(String(warnings[0])).toContain('Semantic writes remain protected');
    console.warn = () => { throw Error('logging failed'); };
    expect(() => sdk.configure({ failClosed: false })).not.toThrow();
  } finally { console.warn = original; }
});
it('a disposal fault overrides a pending policy halt and preserves one original outcome', async () => {
  for (const phase of ['request', 'response']) {
    const halt = { ...pass, halt: true, findings: 1, class: 'semantic' as const };
    const events: string[] = [];
    const sdk = createAgindow({ mode: 'strict', onEvent: e => events.push(e.type) }, async () => ({ begin() {
      return { request: phase === 'request' ? halt : pass, finish: () => halt,
        close() { throw Error('disposal'); } };
    } }));
    await sdk.ready;
    let calls = 0;
    const value = {};
    expect(sdk.guard(() => { calls++; return value; }, { operation: 'write' })()).toBe(value);
    expect(calls).toBe(1);
    expect(events).toEqual(['error', 'bypass']);
  }
});
it('an event callback cannot release a pending semantic write halt', async () => {
  const halt = { ...pass, halt: true, findings: 1, class: 'semantic' as const };
  const sdk = createAgindow({ mode: 'repair', onEvent: event => {
    if (event.verdict !== undefined) Reflect.set(event.verdict, 'halt', false);
  } }, async () => ({ begin() {
    return { request: halt, finish: () => pass, close() {} };
  } }));
  await sdk.ready;
  let calls = 0;
  expect(() => sdk.guard(() => { calls++; }, { operation: 'write' })()).toThrow('halted request');
  expect(calls).toBe(0);
});

it('keeps per-call mode overrides independent and freezes each mode across async settlement', async () => {
  const selected: string[] = [];
  const sdk = createAgindow({ mode: 'observe' }, async () => ({ begin(_value, mode, _operation, _fail, tool) {
    selected.push(`${tool}:request:${mode}`);
    return { request: pass, finish() { selected.push(`${tool}:response:${mode}`); return pass; }, close() {} };
  } })); await sdk.ready;
  let release: (() => void) | undefined;
  const strict = sdk.guard(() => new Promise<void>(resolve => { release = resolve; }), { tool: 'strict', mode: 'strict' });
  const normal = sdk.guard(() => 1, { tool: 'normal' });
  const pending = strict(); expect(normal()).toBe(1); release?.(); await pending; normal();
  expect(selected).toEqual(['strict:request:strict', 'normal:request:observe', 'normal:response:observe',
    'strict:response:strict', 'normal:request:observe', 'normal:response:observe']);
});
it('bypasses excluded tools once without evaluating either phase and lets deny override allow', async () => {
  const selected: string[] = [];
  const sdk = createAgindow({ allowedTools: ['allowed', 'denied'], denyTools: ['denied'] }, async () => ({ begin(_v, _m, _o, _f, tool) {
    selected.push(tool); return { request: pass, finish() { selected.push(`${tool}:response`); return pass; }, close() {} };
  } })); await sdk.ready;
  const value = {};
  for (const tool of ['allowed', 'denied', 'other']) expect(sdk.guard(() => value, { tool })()).toBe(value);
  expect(selected).toEqual(['allowed', 'allowed:response']);
  expect(sdk.recent().map(event => [event.type, event.tool])).toEqual([['bypass', 'denied'], ['bypass', 'other']]);
  sdk.configure({ allowedTools: '*', denyTools: [] });
  sdk.guard(() => value, { tool: 'other' })();
  expect(selected.slice(-2)).toEqual(['other', 'other:response']);
});
