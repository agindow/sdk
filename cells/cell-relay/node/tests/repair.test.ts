import { expect, it } from 'vitest';
import { createAgindow } from '../src/index.js';
import type { Verdict } from '../src/types.js';
const pass: Verdict = { halt: false, bypassed: false, class: null, findings: 0, detectors: 0, malformed: false };
it('applies only engine-verified replacements, reports computed identity and simulates the same changes', async () => {
  const original = { old: 'value' }; const replacement = { current: 'value' };
  const changes = [{ path: 'old', kind: 'field_renamed', to: 'current' }];
  const repair = { ...pass, class: 'shape' as const, findings: 1, confidence: 0.99,
    repair: { verified: true as const, value: replacement, changes } };
  const sdk = createAgindow({ mode: 'repair' }, async () => ({ begin: () => ({ request: pass, finish: () => repair, close() {} }) }));
  await sdk.ready;
  expect(sdk.guard(() => original)()).toBe(replacement);
  expect(original).toEqual({ old: 'value' });
  expect(sdk.recent().find(event => event.type === 'repair')).toMatchObject({ byte_identical: false, confidence: 0.99 });
  expect(sdk.simulate('tool', { recorded: original })).toMatchObject({ status: 'repaired', changes, byte_identical: false });
  sdk.configure({ mode: 'observe' }); expect(sdk.guard(() => original)()).toBe(original);
});
it('dispatches verified request arguments exactly once while retaining all other arguments and receiver', async () => {
  const replacement = { new: 'value' }; const received: unknown[][] = [];
  const sdk = createAgindow({ mode: 'repair' }, async () => ({ begin: () => ({ request: { ...pass,
    class: 'shape' as const, confidence: 0.99, findings: 1, repair: { verified: true as const, value: [replacement, 7], changes: [] } },
    finish: () => pass, close() {} }) })); await sdk.ready;
  const fn = sdk.guard(function(this: object, a: object, b: number) { received.push([this, a, b]); return a; });
  const receiver = {}; expect(fn.call(receiver, {}, 7)).toBe(replacement); expect(received).toEqual([[receiver, replacement, 7]]);
});
it('bypasses invalid or under-threshold replacement evidence rather than applying it', async () => {
  for (const confidence of [0.5, NaN, Infinity, 1.1]) {
    const sdk = createAgindow({ mode: 'strict' }, async () => ({ begin: () => ({ request: pass, finish: () => ({ ...pass,
      class: 'shape' as const, confidence, findings: 1, repair: { verified: true as const, value: {}, changes: [] } }), close() {} }) }));
    await sdk.ready; const original = {}; expect(sdk.guard(() => original)()).toBe(original);
    expect(sdk.recent().filter(event => event.type === 'bypass')).toHaveLength(1);
  }
});
