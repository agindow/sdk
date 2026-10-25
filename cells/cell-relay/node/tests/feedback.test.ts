import { expect, it } from 'vitest';
import { createAgindow, AgindowDriftError } from '../src/index.js';
import type { Verdict, PolicyFeedback } from '../src/types.js';
const pass: Verdict = { halt: false, bypassed: false, class: null, findings: 0, detectors: 0, malformed: false };
const feedback: PolicyFeedback = { error: 'drift_blocked', tool: 'vendor.write', drift_class: 'semantic',
  drift_id: 'drift_fixture', hint: 'Use the documented status vocabulary.', audit: 'https://api.agindow.com/v1/audit/aud_fixture' };
it('projects TI-25 six-field feedback and preserves an original upstream exception as cause', async () => {
  const original = Error('upstream failed');
  const sdk = createAgindow({ mode: 'repair' }, async () => ({ begin: () => ({ request: pass,
    finish: () => ({ ...pass, halt: true, findings: 1, class: 'semantic' as const, feedback }), close() {} }) })); await sdk.ready;
  try { await sdk.guard(() => Promise.reject(original), { tool: feedback.tool, operation: 'write' })(); throw Error('missing halt'); }
  catch (error) {
    expect(error).toBeInstanceOf(AgindowDriftError);
    expect(error).toMatchObject({ code: 'drift_blocked', tool: feedback.tool, driftClass: feedback.drift_class,
      driftId: feedback.drift_id, auditUrl: feedback.audit, cause: original });
    expect((error as Error).message).toContain('halted response');
    if (error instanceof AgindowDriftError) expect(error.hint).toContain('do not blindly replay');
  }
});
it('publishes passthrough feedback separately without replacing the result or exception', async () => {
  const sdk = createAgindow({ mode: 'repair' }, async () => ({ begin: () => ({ request: pass,
    finish: () => ({ ...pass, findings: 1, class: 'shape' as const, feedback: { ...feedback, error: 'drift_passthrough' as const } }), close() {} }) })); await sdk.ready;
  const original = {}; expect(sdk.guard(() => original)()).toBe(original);
  expect(sdk.recent()[0]?.verdict?.feedback).toMatchObject({ error: 'drift_passthrough' });
});
it('delivers an internal error event and exactly one bypass without throwing it', async () => {
  const sdk = createAgindow({}, async () => ({ begin() { throw Error('private engine detail'); } })); await sdk.ready;
  const errors: string[] = []; sdk.on('error', event => errors.push(event.type));
  const original = {}; expect(sdk.guard(() => original)()).toBe(original);
  expect(errors).toEqual(['error']); expect(sdk.recent().filter(event => event.type === 'bypass')).toHaveLength(1);
});
