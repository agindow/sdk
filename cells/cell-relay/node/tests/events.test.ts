import { expect, it, vi } from 'vitest';
import { trace } from '@opentelemetry/api';
import { BasicTracerProvider, InMemorySpanExporter, SimpleSpanProcessor } from '@opentelemetry/sdk-trace-base';
import { createAgindow } from '../src/index.js';
import type { Engine, Event, Verdict } from '../src/types.js';
const pass: Verdict = { halt: false, bypassed: false, class: null, findings: 0, detectors: 0, malformed: false };
const engine: Engine = { begin: () => ({ request: pass, finish: () => pass, close() {} }) };
it('delivers and bounds events without allowing listener faults to affect calls', async () => {
  const sdk = createAgindow({ buffer: 2 }, async () => { throw Error('not ready'); }); await sdk.ready;
  const events: Event[] = [];
  const off = sdk.on('bypass', event => { events.push(event); throw Error('listener'); });
  for (let index = 0; index < 4; index++) expect(sdk.guard(() => index, { tool: `t${index}` })()).toBe(index);
  expect(events).toHaveLength(4);
  expect(sdk.recent().map(event => event.tool)).toEqual(['t2', 't3']);
  expect(events[0]).toMatchObject({ mode: 'observe', side: 'request', phase: 'request', latencyUs: expect.any(Number) });
  off(); sdk.guard(() => 1)(); expect(events).toHaveLength(4);
  sdk.configure({ buffer: 0 }); sdk.guard(() => 1)(); expect(sdk.recent()).toEqual([]);
});
it('freezes published events and verdicts before delivering them to listeners', async () => {
  const halted = { ...pass, halt: true, findings: 1, class: 'semantic' as const };
  const sdk = createAgindow({ mode: 'strict', onEvent(event) {
    Reflect.set(event, 'mode', 'observe');
    if (event.verdict !== undefined) Reflect.set(event.verdict, 'halt', false);
  } }, async () => ({ begin: () => ({ request: halted, finish: () => pass, close() {} }) }));
  await sdk.ready;
  expect(() => sdk.guard(() => 1)()).toThrow();
  expect(sdk.recent().map(event => event.type)).toEqual(['drift', 'blocked']);
  expect(sdk.recent()[1]?.mode).toBe('strict');
});
it('opens and closes sampled OTel spans, records drift and respects spans:false', async () => {
  trace.disable();
  const exporter = new InMemorySpanExporter();
  const provider = new BasicTracerProvider({ spanProcessors: [new SimpleSpanProcessor(exporter)] });
  trace.setGlobalTracerProvider(provider);
  try {
    const sdk = createAgindow({}, async () => engine); await sdk.ready;
    expect(await sdk.guard(() => Promise.resolve(7), { tool: 'vendor.get' })()).toBe(7);
    const halt = { ...pass, halt: true, findings: 1, class: 'semantic' as const };
    const protectedSdk = createAgindow({ mode: 'strict' }, async () => ({ begin: () => ({ request: halt, finish: () => pass, close() {} }) }));
    await protectedSdk.ready;
    expect(() => protectedSdk.guard(() => 0, { tool: 'vendor.write' })()).toThrow();
    await provider.forceFlush();
    const spans = exporter.getFinishedSpans(); expect(spans).toHaveLength(2);
    expect(spans[0]?.attributes['agindow.tool']).toBe('vendor.get');
    expect(spans[1]?.attributes).toMatchObject({ 'agindow.status': 'blocked', 'agindow.drift.class': 'semantic' });
    sdk.guard(() => 0, { spans: false })(); expect(exporter.getFinishedSpans()).toHaveLength(2);
  } finally { trace.disable(); await provider.shutdown(); }
});
it('closes spans on original upstream exceptions and contains instrumentation faults', async () => {
  const sdk = createAgindow({}, async () => engine); await sdk.ready;
  const error = Error('original');
  const start = vi.spyOn(trace, 'getTracer').mockImplementation(() => { throw Error('instrumentation'); });
  try { expect(sdk.guard(() => 3)()).toBe(3); } finally { start.mockRestore(); }
  await expect(sdk.guard(() => Promise.reject(error))()).rejects.toBe(error);
});
