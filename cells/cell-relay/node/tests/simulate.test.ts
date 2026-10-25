import { expect, it, vi } from 'vitest';
import { createAgindow } from '../src/index.js';
import type { Verdict } from '../src/types.js';
const pass: Verdict = { halt: false, bypassed: false, class: null, findings: 0, detectors: 0, malformed: false };
it('dry-runs the same engine phases deterministically without invoking upstream or network', async () => {
  const begin = vi.fn(() => ({ request: pass, finish: () => ({ ...pass, halt: true, findings: 1, class: 'semantic' as const, confidence: 0.99 }), close() {} }));
  const sdk = createAgindow({ mode: 'repair' }, async () => ({ begin })); await sdk.ready;
  const fetch = vi.spyOn(globalThis, 'fetch').mockImplementation(() => { throw Error('network forbidden'); });
  try {
    const input = { recorded: { value: 100 }, args: { value: 1 }, operation: 'write' as const };
    const first = sdk.simulate('vendor.update', input);
    expect(first).toEqual(sdk.simulate('vendor.update', input));
    expect(first).toMatchObject({ status: 'blocked', driftClass: 'semantic', confidence: 0.99, byte_identical: true, wouldWrite: true });
    expect(fetch).not.toHaveBeenCalled(); expect(begin).toHaveBeenCalledTimes(2);
    const upstream = vi.fn(() => input.recorded);
    expect(() => sdk.guard(upstream, { tool: 'vendor.update', operation: 'write' })()).toThrow();
    expect(upstream).toHaveBeenCalledTimes(1);
  } finally { fetch.mockRestore(); }
});
it('reports bypass for partial recordings and faults without claiming clean input or calling the engine', async () => {
  const begin = vi.fn(() => { throw Error('fault'); });
  const sdk = createAgindow({}, async () => ({ begin })); await sdk.ready;
  expect(sdk.simulate('tool', { recorded: '{', partial: true }).status).toBe('bypassed');
  expect(begin).not.toHaveBeenCalled();
  expect(sdk.simulate('tool', { recorded: {} }).status).toBe('bypassed');
});

it('returns deterministic clean, observed, repaired and bypassed verdicts from the same execution seam', async () => {
  const changes = [{ path: '/old', kind: 'field_renamed', to: '/new' }];
  const patched = { new: 1 };
  const cases: readonly { verdict: Verdict; mode: 'observe' | 'repair'; status: string }[] = [
    { verdict: pass, mode: 'repair', status: 'no_drift' },
    { verdict: { ...pass, findings: 1, class: 'shape', confidence: .99 }, mode: 'observe', status: 'observed' },
    { verdict: { ...pass, findings: 1, class: 'shape', confidence: .99,
      repair: { verified: true, value: patched, changes } }, mode: 'repair', status: 'repaired' },
    { verdict: { ...pass, bypassed: true }, mode: 'repair', status: 'bypassed' },
  ];
  const network = vi.spyOn(globalThis, 'fetch').mockImplementation(() => { throw Error('network forbidden'); });
  try {
    for (const item of cases) {
      const sdk = createAgindow({ mode: item.mode }, async () => ({ begin: () => ({ request: pass, finish: () => item.verdict, close() {} }) }));
      await sdk.ready;
      const recorded = { old: 1 }; const input = { recorded, operation: 'read' as const };
      const result = sdk.simulate('vendor.lookup', input);
      expect(result).toEqual(sdk.simulate('vendor.lookup', input));
      expect(result.status).toBe(item.status); expect(result.wouldWrite).toBe(false);
      const actual = sdk.guard(() => recorded)();
      expect(actual).toBe(item.status === 'repaired' ? patched : recorded);
      expect(result.changes).toEqual(item.status === 'repaired' ? changes : []);
      expect(result.byte_identical).toBe(item.status !== 'repaired');
      expect(recorded).toEqual({ old: 1 });
    }
    expect(network).not.toHaveBeenCalled();
  } finally { network.mockRestore(); }
});
it('reports deterministic degraded and conservative malformed sandbox recordings through the public API', async () => {
  const sdk = createAgindow({ mode: 'repair' }); await sdk.ready;
  const contract = { version: 1 as const, id: 'fixture', file: 'fixture.ts', tool: 'vendor.lookup',
    method: 'get' as const, endpoint: '/fixture', mode: 'repair' as const, operation: 'read' as const,
    response: { fields: { status: { type: 'string' as const, required: true } } } };
  const degraded = { recorded: {}, contract };
  expect(sdk.simulate('vendor.lookup', degraded)).toEqual(sdk.simulate('vendor.lookup', degraded));
  expect(sdk.simulate('vendor.lookup', degraded)).toMatchObject({ status: 'degraded', driftClass: 'shape',
    changes: [{ path: 'status', kind: 'required_added' }], confidence: null, byte_identical: true, wouldWrite: false });
  expect(sdk.simulate('vendor.lookup', { contract, recorded: '{' }).status).toBe('bypassed');
});
