import { createHash } from 'node:crypto';
import type { AgentInput, Case, Corpus, Json, Payload } from './types.js';

export const classes = ['unit_flip', 'timestamp_epoch', 'enum_expansion', 'null_semantics', 'type_coercion', 'date_format', 'field_rename'] as const;
const scenarios = ['repairable', 'misleading', 'ambiguous', 'unsupported'] as const;
export const hash = (bytes: string): string => createHash('sha256').update(bytes).digest('hex');
// Canonical object key order makes equality independent of an adapter's serializer.
export function canonical(value: Json | object): string {
  if (value === null || typeof value !== 'object') return JSON.stringify(value);
  if (Array.isArray(value)) return `[${value.map(v => canonical(v as Json)).join(',')}]`;
  return `{${Object.entries(value).sort(([a], [b]) => a.localeCompare(b, 'en')).map(([k, v]: [string, Json]) => `${JSON.stringify(k)}:${canonical(v)}`).join(',')}}`;
}
export const corpusHash = (corpus: Corpus): string => hash(canonical(corpus));
export function agentInput(c: Case): AgentInput {
  return structuredClone({ id: c.id, payload: c.payload, finding: c.finding });
}

export function development(version = 'development-v1', split: Corpus['split'] = 'development', seed = 'public-v1'): Corpus {
  const cases: Case[] = [];
  for (const kind of classes) for (let variant = 1; variant <= 10; variant++) for (const scenario of scenarios) {
    const token = hash(`${seed}:${kind}:${variant}:${scenario}`);
    const n = Number.parseInt(token.slice(0, 6), 16) + 1;
    const recipes: Record<typeof kind, [Json, Json]> = {
      unit_flip: [n, n * 1000], timestamp_epoch: [1700000000 + n, (1700000000 + n) * 1000],
      enum_expansion: ['legacy', `current-${token.slice(0, 8)}`], null_semantics: ['', `explicit-${token.slice(0, 8)}`],
      type_coercion: [String(n), n], date_format: [`22/10/${2020 + variant}`, `${2020 + variant}-10-22`],
      field_rename: [n, n],
    };
    const [old, intended] = recipes[kind];
    const source = kind === 'field_rename' ? 'old_value' : 'value';
    const payload: Payload = { [source]: old, memo: `caller-${token.slice(8, 20)}`, context: { request: token.slice(20), dryRun: false } };
    const expected: Payload = { value: intended, memo: payload.memo ?? null, context: payload.context ?? null };
    const proposal = scenario === 'misleading' ? (typeof intended === 'number' ? intended + 1 : `${intended}-wrong`) : intended;
    cases.push({
      id: `${split}:${kind}:${variant}:${scenario}`, class: kind, scenario,
      provenance: 'synthetic', origin: 'authored-without-private-evidence', payload,
      finding: scenario === 'unsupported' ? null : { source, target: 'value', proposal, confidence: scenario === 'ambiguous' ? 0.2 : 0.99 },
      oracle: { expected, vendor: { target: 'value', type: typeof intended === 'number' ? 'number' : 'string', allowed: null },
        rationale: `${kind}/${scenario}: authored migration intent; caller memo and context must survive. Confidence is a proposal, not ground truth.` },
    });
  }
  return { version, split, cases };
}

export function object(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}
export function json(value: unknown): value is Json {
  if (value === null || typeof value === 'string' || typeof value === 'boolean') return true;
  if (typeof value === 'number') return Number.isFinite(value);
  if (Array.isArray(value)) return value.every(json);
  return object(value) && Object.values(value).every(json);
}
export function validateCorpus(value: unknown): asserts value is Corpus {
  if (!object(value) || typeof value.version !== 'string' || !/^[a-z0-9-]{1,80}$/.test(value.version)
    || !['development', 'heldout'].includes(String(value.split)) || !Array.isArray(value.cases) || value.cases.length === 0) throw new Error('Invalid corpus');
  const ids = new Set<string>();
  for (const c of value.cases) {
    if (!object(c) || typeof c.id !== 'string' || !c.id || ids.has(c.id) || !classes.some(k => k === c.class)
      || !scenarios.some(k => k === c.scenario) || c.provenance !== 'synthetic' || c.origin !== 'authored-without-private-evidence'
      || !object(c.payload) || !json(c.payload) || !object(c.oracle) || !object(c.oracle.expected) || !json(c.oracle.expected)
      || typeof c.oracle.rationale !== 'string' || !c.oracle.rationale || !object(c.oracle.vendor)) throw new Error('Invalid or private case');
    const vendor = c.oracle.vendor;
    if (typeof vendor.target !== 'string' || !['number', 'string'].includes(String(vendor.type))
      || !(vendor.allowed === null || (Array.isArray(vendor.allowed) && vendor.allowed.every(json)))
      || typeof c.oracle.expected[vendor.target] !== vendor.type) throw new Error('Invalid oracle/vendor');
    if (c.finding !== null && (!object(c.finding) || typeof c.finding.source !== 'string' || typeof c.finding.target !== 'string'
      || !json(c.finding.proposal) || typeof c.finding.confidence !== 'number' || !Number.isFinite(c.finding.confidence)
      || c.finding.confidence < 0 || c.finding.confidence > 1)) throw new Error('Invalid finding');
    ids.add(c.id);
  }
}
