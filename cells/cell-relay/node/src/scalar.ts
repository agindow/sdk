import { AgindowSetupError } from './types.js';
import { factLabel } from './repair-facts.js';
import { parentParts } from './rename-preservation.js';
import { scalarToken } from './scalar-tokens.js';
import { decimal, scaled } from './exact.js';
export interface ScalarScope {
  readonly version: 1; readonly tool: string; readonly side: 'request' | 'response';
  readonly contract: string; readonly migration: string; readonly provenance: string;
  readonly expiresAt: number; readonly pointer: string;
}
export type ScalarFacts = ScalarScope & (
  | {readonly recipe: 'string-map'; readonly mapping: readonly (readonly [string, string])[]; readonly equivalence?: readonly (readonly [string, string])[]}
  | {readonly recipe: 'empty-null'; readonly nullMeaning: string}
  | {readonly recipe: 'scale'; readonly sourceUnit: string; readonly targetUnit: string;
    readonly numerator: string; readonly denominator: string; readonly inputMin: string; readonly inputMax: string;
    readonly outputMin: string; readonly outputMax: string}
);
export interface ScalarRule {
  readonly recipe: 'scalar'; readonly tool: string; readonly side: 'request' | 'response';
  readonly migration: string; readonly confidence: number; readonly facts: ScalarFacts;
}
const ratios: Readonly<Record<string, readonly [string, string]>> = {
  'cents/dollars': ['1','100'], 'dollars/cents': ['100','1'],
  'seconds/milliseconds': ['1000','1'], 'milliseconds/seconds': ['1','1000'],
  'meters/millimeters': ['1000','1'], 'millimeters/meters': ['1','1000'],
};
export function admitScalar(rule: ScalarRule): ScalarRule {
  try {
    const f = rule.facts;
    if (rule.recipe !== 'scalar' || !f || f.version !== 1 || !factLabel(rule.tool) || !factLabel(rule.migration)
      || !['request','response'].includes(rule.side) || f.tool !== rule.tool || f.side !== rule.side || f.migration !== rule.migration
      || !factLabel(f.contract) || !factLabel(f.provenance) || !Number.isSafeInteger(f.expiresAt) || f.expiresAt <= 0
      || !Number.isFinite(rule.confidence) || rule.confidence < 0 || rule.confidence > 1 || typeof f.pointer !== 'string' || f.pointer === '') throw Error('scope');
    if ([rule.tool, rule.migration, f.contract, f.provenance].some(s => Buffer.byteLength(s) > 256)) throw Error('scope byte limit');
    parentParts(f.pointer);
    const keys = ['version','recipe','tool','side','contract','migration','provenance','expiresAt','pointer'];
    if (f.recipe === 'string-map') {
      keys.push('mapping','equivalence');
      if (!Array.isArray(f.mapping) || f.mapping.length < 1 || f.mapping.length > 64) throw Error('mapping limit');
      const sources = new Set<string>();
      for (const pair of f.mapping) {
        if (!Array.isArray(pair) || pair.length !== 2) throw Error('mapping pair');
        const [source,target] = pair;
        if (typeof source !== 'string' || typeof target !== 'string' || Buffer.byteLength(source) > 256 || Buffer.byteLength(target) > 256
          || /[\uD800-\uDFFF]/u.test(source + target) || source === target || sources.has(source)) throw Error('mapping source');
        sources.add(source);
      }
      const reviews = new Map<string,string>();
      if (f.equivalence !== undefined) {
        if (!Array.isArray(f.equivalence) || f.equivalence.length > 64) throw Error('equivalence bound');
        for (const pair of f.equivalence) {
          if (!Array.isArray(pair) || pair.length !== 2 || !sources.has(pair[0]) || !factLabel(pair[1]) || Buffer.byteLength(pair[1]) > 256 || reviews.has(pair[0])) throw Error('equivalence review');
          reviews.set(pair[0], pair[1]);
        }
      }
      for (const [source,target] of f.mapping) {
        if (f.mapping.filter(([,t]) => t === target).length > 1 && !reviews.has(source)) throw Error('missing source equivalence review');
      }
    } else if (f.recipe === 'empty-null') {
      keys.push('nullMeaning'); if (!factLabel(f.nullMeaning)) throw Error('null meaning');
    } else if (f.recipe === 'scale') {
      keys.push('sourceUnit','targetUnit','numerator','denominator','inputMin','inputMax','outputMin','outputMax');
      const ratio = ratios[`${f.sourceUnit}/${f.targetUnit}`];
      if (!ratio || f.numerator !== ratio[0] || f.denominator !== ratio[1]
        || decimal(f.inputMin) > decimal(f.inputMax) || decimal(f.outputMin) > decimal(f.outputMax)) throw Error('scale facts');
    } else throw Error('recipe');
    if (Object.keys(f).some(key => !keys.includes(key))) throw Error('unsupported facts');
    const facts: ScalarFacts = f.recipe === 'string-map' ? {...f, mapping: Object.freeze(f.mapping.map(pair => Object.freeze([...pair]) as readonly [string,string])),
      ...(f.equivalence === undefined ? {} : {equivalence: Object.freeze(f.equivalence.map(pair => Object.freeze([...pair]) as readonly [string,string]))})} : {...f};
    return Object.freeze({recipe: 'scalar', tool: rule.tool, side: rule.side, migration: rule.migration, confidence: rule.confidence, facts: Object.freeze(facts)});
  } catch { throw new AgindowSetupError('invalid scalar repair: require scoped reviewed exact recipe facts'); }
}
export function scalarPlan(before: string, facts: ScalarFacts): {after: string; changes: readonly {path: string; kind: 'replaced'}[]} {
  const token = scalarToken(before, facts.pointer); let replacement: string;
  if (facts.recipe === 'string-map') {
    const value: unknown = JSON.parse(token.raw); const pair = facts.mapping.find(([source]) => source === value);
    if (typeof value !== 'string' || !pair) throw Error('unknown mapping value');
    replacement = JSON.stringify(pair[1]);
  } else if (facts.recipe === 'empty-null') {
    if (token.raw !== '""') throw Error('not actually empty'); replacement = 'null';
  } else {
    const input = decimal(token.raw); replacement = scaled(token.raw, facts.numerator, facts.denominator); const output = decimal(replacement);
    if (input < decimal(facts.inputMin) || input > decimal(facts.inputMax) || output < decimal(facts.outputMin) || output > decimal(facts.outputMax)) throw Error('domain bounds');
  }
  return {after: before.slice(0, token.start) + replacement + before.slice(token.end), changes: [{path: token.path, kind: 'replaced'}]};
}
