import { scalarToken } from './scalar-tokens.js';
import { decimal } from './exact.js';
import { admitScalar, scalarPlan } from './scalar.js';
import type { ScalarRule } from './scalar.js';
import { types } from 'node:util';
import { AgindowSetupError } from './types.js';
import type { Engine, EngineCall, Mode, Operation, Verdict } from './types.js';
import { capture } from './inspection.js';
import { renamePlan, selectedParents } from './rename-preservation.js';
import { RawJson } from './raw.js';
import { admitFacts, factLabel, factsMatch } from './repair-facts.js';
import type { RepairFacts } from './repair-facts.js';
export interface RenameRule {
  readonly facts?: RepairFacts;
  readonly tool: string;
  readonly side: 'request' | 'response';
  readonly from: string;
  readonly to: string;
  /** Reviewed migration identity, independent of payload/schema guesses. */
  readonly migration: string;
  readonly confidence: number;
  /** Concrete RFC 6901 object-parent pointers; absent means the legacy root key. */
  readonly parents?: readonly string[];
  readonly contract?: string;
  readonly provenance?: string;
  /** Unix milliseconds; mandatory for nested selections. */
  readonly expiresAt?: number;
}
export type RepairRule = RenameRule | ScalarRule;
export interface RepairCheck {
  readonly before: string; readonly after: string; readonly rule: RepairRule;
}
export interface SelectiveRepairOptions {
  readonly rules: readonly RepairRule[];
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
    if (rule === null || typeof rule !== 'object') return fail();
    if ('recipe' in rule) {
      const admitted = admitScalar(rule);
      const scope = JSON.stringify([rule.tool, rule.side]); if (scopes.has(scope)) return fail(); scopes.add(scope);
      return admitted;
    }
    if (rule.parents !== undefined) {
      try {
        if (!Array.isArray(rule.parents) || rule.parents.some((p: unknown) => typeof p !== 'string')
          || typeof rule.contract !== 'string' || !/^[^\u0000-\u001f\u007f]{1,256}$/u.test(rule.contract)
          || typeof rule.provenance !== 'string' || !/^[^\u0000-\u001f\u007f]{1,256}$/u.test(rule.provenance)
          || !Number.isSafeInteger(rule.expiresAt) || (rule.expiresAt ?? 0) <= 0) return fail();
        selectedParents(rule.parents, rule.from);
      } catch { return fail(); }
    }
    const key = (v: string): boolean => rule.parents === undefined ? /^[A-Za-z_][A-Za-z_0-9]{0,63}$/u.test(v) : Buffer.byteLength(v) <= 64 && !/[\u0000-\u001f\u007f\uD800-\uDFFF]/u.test(v);
    if (rule === null || typeof rule !== 'object' || !['request', 'response'].includes(rule.side)
      || typeof rule.tool !== 'string' || !/^[^\u0000-\u001f\u007f]{1,256}$/u.test(rule.tool)
      || typeof rule.migration !== 'string' || !/^[^\u0000-\u001f\u007f]{1,256}$/u.test(rule.migration)
      || typeof rule.from !== 'string' || typeof rule.to !== 'string' || !key(rule.from)
      || !key(rule.to) || rule.from === rule.to
      || !Number.isFinite(rule.confidence) || rule.confidence < 0 || rule.confidence > 1) return fail();
    if ((rule.contract !== undefined && !factLabel(rule.contract)) || (rule.provenance !== undefined && !factLabel(rule.provenance))
      || (rule.expiresAt !== undefined && (!Number.isSafeInteger(rule.expiresAt) || rule.expiresAt <= 0))) return fail();
    const facts = rule.facts === undefined ? undefined : admitFacts(rule.facts, rule);
    const scope = JSON.stringify([rule.tool, rule.side]); if (scopes.has(scope)) return fail(); scopes.add(scope);
    return Object.freeze({ tool: rule.tool, side: rule.side, from: rule.from, to: rule.to, migration: rule.migration, confidence: rule.confidence,
      ...(rule.parents === undefined ? {} : { parents: Object.freeze([...rule.parents]) }),
      ...(rule.contract === undefined ? {} : {contract: rule.contract}),
      ...(rule.provenance === undefined ? {} : {provenance: rule.provenance}),
      ...(rule.expiresAt === undefined ? {} : {expiresAt: rule.expiresAt}),
      ...(facts === undefined ? {} : {facts}) });
  });
  return Object.freeze({ rules: Object.freeze(rules), verify: value.verify });
}
export type PrepareRename = (before: string, rule: RepairRule, mode: Mode) => string;
/** Composes preparation with the existing engine; never clears its findings or faults. */
export function selectiveEngine(base: Engine, options: SelectiveRepairOptions, prepare: PrepareRename): Engine {
  const apply = (value: unknown, verdict: Verdict, rule: RepairRule | undefined, mode: Mode, operation: Operation): Verdict => {
    if (!rule || verdict.halt || verdict.bypassed || verdict.findings > 0 || verdict.malformed) return verdict;
    const input = capture(value); if (input.truncated) throw Error('repair capture unavailable');
    const raw: unknown = JSON.parse(prepare(input.json, rule, mode));
    if (typeof raw !== 'object' || raw === null || !('matched' in raw) || typeof raw.matched !== 'boolean'
      || !('applied' in raw) || typeof raw.applied !== 'boolean') throw Error('invalid repair preparation');
    if (!raw.matched) return verdict;
    if (!('halt' in raw) || typeof raw.halt !== 'boolean') throw Error('invalid repair policy');
    const scalar = 'recipe' in rule;
    const expiresAt = scalar ? rule.facts.expiresAt : rule.expiresAt;
    const found: Verdict = { ...verdict, class: scalar ? 'semantic' : 'shape', findings: 1, confidence: rule.confidence, halt: scalar ? mode !== 'observe' && operation !== 'read' : raw.halt,
      ...(scalar && !raw.applied ? {repairRefusal: 'reason' in raw && typeof raw.reason === 'string' ? raw.reason : 'unavailable-candidate'} : {}) };
    if (scalar && rule.confidence < .99) return {...found, repairRefusal: 'confidence-or-policy'};
    if (!raw.applied || mode === 'observe') return found;
    if (expiresAt !== undefined && Date.now() >= expiresAt) return scalar ? {...found, repairRefusal: 'stale-facts'} : found;
    if (!('body' in raw) || typeof raw.body !== 'string') throw Error('invalid repair bytes');
    const plan = scalar ? scalarPlan(input.json, rule.facts) : renamePlan(input.json, rule.from, rule.to, rule.parents ?? ['']);
    if (plan.after !== raw.body) throw Error('repair preservation verification failed');
    if (!scalar && rule.facts !== undefined && !factsMatch(input.json, raw.body, rule.facts)) return found;
    const verified: unknown = options.verify(Object.freeze({ before: input.json, after: raw.body, rule }));
    if (typeof verified !== 'boolean') {
      if (types.isPromise(verified)) void verified.catch(() => undefined);
      throw Error('repair verifier must return a synchronous boolean');
    }
    if (!verified || (rule.facts !== undefined && Date.now() >= rule.facts.expiresAt)
      || (expiresAt !== undefined && Date.now() >= expiresAt)) return scalar ? {...found, repairRefusal: !verified ? 'verifier-refused' : 'stale-facts'} : found;
    const replacement: unknown = value instanceof RawJson ? new RawJson(new TextEncoder().encode(raw.body)) : JSON.parse(raw.body);
    if (scalar && rule.facts.recipe === 'scale' && !(value instanceof RawJson)) {
      const projected = capture(replacement);
      if (projected.truncated || decimal(scalarToken(projected.json, rule.facts.pointer).raw) !== decimal(scalarToken(raw.body, rule.facts.pointer).raw)) return {...found, repairRefusal: 'projection-precision'};
    }
    return { ...found, halt: false, repair: { verified: true, value: replacement,
      changes: scalar ? plan.changes.map(change => ({path: change.path, kind: 'replaced'})) : plan.changes.map(change => ({...change, kind: 'renamed'})) } };
  };
  return { ...base,
    begin(value, mode, operation, failClosed, tool): EngineCall {
      const call = base.begin(value, mode, operation, failClosed, tool);
      try {
        const request = apply(value, call.request, options.rules.find(rule => rule.tool === tool && rule.side === 'request'), mode, operation);
        return { request, close: () => call.close(),
          finish(response) { return apply(response, call.finish(response), options.rules.find(rule => rule.tool === tool && rule.side === 'response'), mode, operation); } };
      } catch (error) { call.close(); throw error; }
    },
  };
}
