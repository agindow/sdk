import { AgindowSetupError } from './types.js';
import type { RenameRule } from './selective.js';
import { parentParts } from './rename-preservation.js';
export type RepairValueType = 'string' | 'number' | 'boolean' | 'null' | 'object' | 'array';
export interface RepairConstraint { readonly key: string; readonly type: RepairValueType }
/** Host-loaded authority. Never constructed from a detector proposal. */
export interface RepairFacts {
  readonly version: 1;
  readonly recipe: 'rename';
  readonly tool: string;
  readonly side: 'request' | 'response';
  readonly contract: string;
  readonly migration: string;
  readonly provenance: string;
  readonly expiresAt: number;
  readonly parents: readonly string[];
  readonly old: RepairConstraint;
  readonly new: RepairConstraint;
}
export const factLabel = (value: unknown): value is string => typeof value === 'string' && /^[^\u0000-\u001f\u007f]{1,256}$/u.test(value);
export function admitFacts(facts: RepairFacts, rule: RenameRule): RepairFacts {
  const fail = (): never => { throw new AgindowSetupError('invalid selectiveRepair facts: require versioned, scoped, independent reviewed constraints'); };
  if (facts === null || typeof facts !== 'object' || facts.version !== 1 || facts.recipe !== 'rename'
    || facts.tool !== rule.tool || facts.side !== rule.side || facts.migration !== rule.migration
    || !factLabel(facts.contract) || !factLabel(facts.provenance)
    || !Number.isSafeInteger(facts.expiresAt) || facts.expiresAt <= 0
    || !Array.isArray(facts.parents) || JSON.stringify(facts.parents) !== JSON.stringify(rule.parents ?? [''])
    || (rule.contract !== undefined && rule.contract !== facts.contract)
    || (rule.provenance !== undefined && rule.provenance !== facts.provenance)
    || (rule.expiresAt !== undefined && rule.expiresAt !== facts.expiresAt)
    || !facts.old || !facts.new || facts.old.key !== rule.from || facts.new.key !== rule.to
    || !['string','number','boolean','null','object','array'].includes(facts.old.type)
    || facts.old.type !== facts.new.type) return fail();
  return Object.freeze({version: 1, recipe: 'rename', tool: facts.tool, side: facts.side, contract: facts.contract,
    migration: facts.migration, provenance: facts.provenance, expiresAt: facts.expiresAt, parents: Object.freeze([...facts.parents]),
    old: Object.freeze({key: facts.old.key, type: facts.old.type}), new: Object.freeze({key: facts.new.key, type: facts.new.type})});
}
/** Called only after the bounded token walk establishes complete, unique JSON. */
export function factsMatch(before: string, after: string, facts: RepairFacts): boolean {
  if (Date.now() >= facts.expiresAt) return false;
  const documents: readonly unknown[] = [JSON.parse(before), JSON.parse(after)];
  return documents.every((document, index) => facts.parents.every(pointer => {
    let value: unknown = document;
    for (const part of parentParts(pointer)) {
      if (typeof value !== 'object' || value === null || !Object.hasOwn(value, part)) return false;
      value = (value as Record<string, unknown>)[part];
    }
    if (typeof value !== 'object' || value === null || Array.isArray(value)) return false;
    const constraint = index === 0 ? facts.old : facts.new;
    const absent = index === 0 ? facts.new.key : facts.old.key;
    if (!Object.hasOwn(value, constraint.key) || Object.hasOwn(value, absent)) return false;
    const member: unknown = (value as Record<string, unknown>)[constraint.key];
    const type = member === null ? 'null' : Array.isArray(member) ? 'array' : typeof member;
    return type === constraint.type;
  }));
}
