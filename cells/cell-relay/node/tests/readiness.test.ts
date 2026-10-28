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
it('default packaged guest uses statistical readiness without selected windows', async () => {
  const { loadWasm } = await import('../dist/esm/wasm.js');
  const engine = await loadWasm({});
  const original = { value: 100 };
  const clean = engine.begin(original, 'repair', 'write', true, 'op');
  expect(clean.request).toMatchObject({ bypassed: false, halt: false, findings: 0, detectors: 0 });
  clean.close();
  engine.setReady?.(false);
  const cold = engine.begin(original, 'repair', 'write', true, 'op');
  expect(cold.request).toMatchObject({ bypassed: true, halt: false, findings: 0 });
  cold.close();
});
