import { types } from 'node:util';
import { AgindowSetupError } from './types.js';
import type { Engine, EngineCall, Mode, Verdict } from './types.js';
import { capture } from './inspection.js';
import { RawJson } from './raw.js';
export interface RenameRule {
  readonly tool: string;
  readonly side: 'request' | 'response';
  readonly from: string;
  readonly to: string;
  /** Reviewed migration identity, independent of payload/schema guesses. */
  readonly migration: string;
  readonly confidence: number;
}
export interface RepairCheck {
  readonly before: string; readonly after: string; readonly rule: RenameRule;
}
export interface SelectiveRepairOptions {
  readonly rules: readonly RenameRule[];
  /** Local synchronous verification against independent authoritative evidence. */
  readonly verify: (check: RepairCheck) => boolean;
}
export function repairOptions(value: SelectiveRepairOptions | false | undefined, audited: boolean): SelectiveRepairOptions | undefined {
  if (value === undefined || value === false) return undefined;
  const fail = (): never => { throw new AgindowSetupError('invalid selectiveRepair: provide 1–16 scoped reviewed rename rules and a synchronous verifier; guest audit cannot be combined'); };
  if (value === null || typeof value !== 'object' || !Array.isArray(value.rules) || value.rules.length < 1 || value.rules.length > 16
    || typeof value.verify !== 'function' || audited) return fail();
  const scopes = new Set<string>();
  const rules = value.rules.map(rule => {
    if (rule === null || typeof rule !== 'object' || !['request', 'response'].includes(rule.side)
      || typeof rule.tool !== 'string' || !/^[^\u0000-\u001f\u007f]{1,256}$/u.test(rule.tool)
      || typeof rule.migration !== 'string' || !/^[^\u0000-\u001f\u007f]{1,256}$/u.test(rule.migration)
      || typeof rule.from !== 'string' || typeof rule.to !== 'string' || !/^[A-Za-z_][A-Za-z_0-9]{0,63}$/u.test(rule.from)
      || !/^[A-Za-z_][A-Za-z_0-9]{0,63}$/u.test(rule.to) || rule.from === rule.to
      || !Number.isFinite(rule.confidence) || rule.confidence < 0 || rule.confidence > 1) return fail();
    const scope = JSON.stringify([rule.tool, rule.side]); if (scopes.has(scope)) return fail(); scopes.add(scope);
    return Object.freeze({ tool: rule.tool, side: rule.side, from: rule.from, to: rule.to, migration: rule.migration, confidence: rule.confidence });
  });
  return Object.freeze({ rules: Object.freeze(rules), verify: value.verify });
}
export type PrepareRename = (before: string, rule: RenameRule, mode: Mode) => string;
/** Composes preparation with the existing engine; never clears its findings or faults. */
export function selectiveEngine(base: Engine, options: SelectiveRepairOptions, prepare: PrepareRename): Engine {
  const apply = (value: unknown, verdict: Verdict, rule: RenameRule | undefined, mode: Mode): Verdict => {
    if (!rule || verdict.halt || verdict.bypassed || verdict.findings > 0 || verdict.malformed) return verdict;
    const input = capture(value); if (input.truncated) throw Error('repair capture unavailable');
    const raw: unknown = JSON.parse(prepare(input.json, rule, mode));
    if (typeof raw !== 'object' || raw === null || !('matched' in raw) || typeof raw.matched !== 'boolean'
      || !('applied' in raw) || typeof raw.applied !== 'boolean') throw Error('invalid repair preparation');
    if (!raw.matched) return verdict;
    if (!('halt' in raw) || typeof raw.halt !== 'boolean') throw Error('invalid repair policy');
    const found: Verdict = { ...verdict, class: 'shape', findings: 1, confidence: rule.confidence, halt: raw.halt };
    if (!raw.applied || mode === 'observe') return found;
    if (!('body' in raw) || typeof raw.body !== 'string') throw Error('invalid repair bytes');
    const verified: unknown = options.verify(Object.freeze({ before: input.json, after: raw.body, rule }));
    if (typeof verified !== 'boolean') {
      if (types.isPromise(verified)) void verified.catch(() => undefined);
      throw Error('repair verifier must return a synchronous boolean');
    }
    if (!verified) return found;
    const replacement: unknown = value instanceof RawJson ? new RawJson(new TextEncoder().encode(raw.body)) : JSON.parse(raw.body);
    return { ...found, halt: false, repair: { verified: true, value: replacement,
      changes: [{ path: rule.from, kind: 'renamed', to: rule.to }] } };
  };
  return { ...base,
    begin(value, mode, operation, failClosed, tool): EngineCall {
      const call = base.begin(value, mode, operation, failClosed, tool);
      try {
        const request = apply(value, call.request, options.rules.find(rule => rule.tool === tool && rule.side === 'request'), mode);
        return { request, close: () => call.close(),
          finish(response) { return apply(response, call.finish(response), options.rules.find(rule => rule.tool === tool && rule.side === 'response'), mode); } };
      } catch (error) { call.close(); throw error; }
    },
  };
}
