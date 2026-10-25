import { expect, it } from 'vitest';
import { bindGuest } from '../src/wasm.js';
it('forwards explicit readiness changes through the production binding', () => {
  const seen: boolean[] = [];
  const engine = bindGuest({ active: () => true, set_ready(value: boolean) { seen.push(value); }, begin() { throw Error('unused'); } }, {});
  engine.setReady?.(false); engine.setReady?.(true);
  expect(seen).toEqual([false, true]);
});
it('rejects malformed diagnostic metadata and freezes detector evidence', () => {
  for (const detectorKinds of [['Unit'], ['invented'], 'Unit']) {
    const json = JSON.stringify({ halt: false, bypassed: false, class: null, findings: 0, detectors: 1,
      malformed: false, detectorKinds });
    const engine = bindGuest({ active: () => false, begin() { return { request: () => json, finish: () => json, free() {} }; } }, {});
    if (Array.isArray(detectorKinds) && detectorKinds[0] === 'Unit') {
      const call = engine.begin({}, 'observe', 'read', true, 'op');
      expect(Object.isFrozen(call.request.detectorKinds)).toBe(true); call.close();
    } else expect(() => engine.begin({}, 'observe', 'read', true, 'op')).toThrow('invalid guest');
  }
});
