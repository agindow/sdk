/** Self-contained sandbox runtime. Codegen embeds this source without imports.
 * Parsed values cannot prove raw byte identity; no numeric conversion exists. */
export interface SandboxFields {
  readonly fields: Readonly<Record<string, { readonly type: 'string' | 'number' | 'boolean' | 'null'; readonly required?: boolean; readonly enum?: readonly string[] }>>;
  readonly renames?: readonly { readonly from: string; readonly to: string; readonly identity: string; readonly confidence: number }[];
}
export interface SandboxContract {
  readonly version: 1;
  readonly id: string;
  readonly file: string;
  readonly tool: string;
  readonly method: 'get' | 'post' | 'put' | 'patch' | 'delete' | 'request';
  readonly endpoint: string;
  readonly mode: 'observe' | 'repair' | 'strict';
  readonly operation: 'read' | 'write' | 'unknown';
  readonly disabled?: boolean;
  readonly request?: SandboxFields;
  readonly response?: SandboxFields;
}
export interface SandboxResult {
  readonly status: 'ok' | 'observed' | 'repaired' | 'degraded' | 'blocked' | 'bypassed';
  readonly driftClass: 'shape' | 'semantic' | 'unknown' | null;
  readonly confidence: number | null;
  readonly changes: readonly { readonly path: string; readonly kind: string; readonly to?: string }[];
  readonly byte_identical: boolean;
  readonly value: unknown;
}
export function evaluateSandbox(contract: SandboxContract, value: unknown, side: 'request' | 'response', partial = false): SandboxResult {
  const clean: SandboxResult = { status: 'ok', driftClass: null, confidence: null, changes: [], byte_identical: true, value };
  try {
    if (partial) return { ...clean, status: 'bypassed' };
    if (contract.disabled === true) return clean;
    const schema = contract[side]; if (schema === undefined) return clean;
    if (value === null || typeof value !== 'object' || Array.isArray(value)) return { ...clean, status: 'bypassed' };
    const prototype: unknown = Object.getPrototypeOf(value);
    if (prototype !== null && typeof prototype !== 'object') return { ...clean, status: 'bypassed' };
    // Sandbox hosts may inject JSON from another realm. Its root prototype is
    // still terminal; class instances have an additional prototype layer.
    if (prototype !== null && Object.getPrototypeOf(prototype) !== null) return { ...clean, status: 'bypassed' };
    const descriptors = Object.getOwnPropertyDescriptors(value);
    if (Object.keys(descriptors).length > 256 || Object.values(descriptors).some(item => !('value' in item))) return { ...clean, status: 'bypassed' };
    let driftClass: SandboxResult['driftClass'] = null;
    let confidence = 1; let unresolved = false;
    const changes: { path: string; kind: string; to?: string }[] = [];
    const staged = { ...descriptors };
    const kind = (input: unknown): string => input === null ? 'null' : typeof input;
    const mark = (classification: 'shape' | 'semantic' | 'unknown'): void => {
      if (driftClass !== 'semantic' && (classification !== 'shape' || driftClass === null)) driftClass = classification;
    };
    for (const rename of side === 'request' ? (schema.renames ?? []) : []) {
      // Descriptor maps inherit Object.prototype; only payload-owned keys count.
      const source = Object.hasOwn(staged, rename.from) ? staged[rename.from] : undefined; if (source === undefined) continue;
      changes.push({ path: rename.from, kind: 'field_renamed', to: rename.to });
      const targetType = Object.hasOwn(schema.fields, rename.to) ? schema.fields[rename.to]?.type : undefined;
      if (targetType === undefined || kind(source.value) !== targetType) { mark('semantic'); unresolved = true; continue; }
      mark('shape');
      if (rename.identity.length === 0 || Object.hasOwn(staged, rename.to) || !Number.isFinite(rename.confidence)
        || rename.confidence < (contract.mode === 'strict' ? 0.99 : 0.97) || rename.confidence > 1) { unresolved = true; continue; }
      confidence = Math.min(confidence, rename.confidence);
      delete staged[rename.from];
      Object.defineProperty(staged, rename.to, { value: source, enumerable: true, writable: true, configurable: true });
    }
    for (const [field, rule] of Object.entries(schema.fields)) {
      const descriptor = Object.hasOwn(staged, field) ? staged[field] : undefined;
      if (descriptor === undefined) {
        if (rule.required === true) { changes.push({ path: field, kind: 'required_added' }); mark('shape'); unresolved = true; }
        continue;
      }
      const input: unknown = descriptor.value;
      if (kind(input) !== rule.type || (typeof input === 'number' && !Number.isFinite(input))) {
        changes.push({ path: field, kind: 'type_changed' }); mark('semantic'); unresolved = true;
      } else if (rule.enum !== undefined && (typeof input !== 'string' || !rule.enum.includes(input))) {
        changes.push({ path: field, kind: 'enum_expanded' }); mark('semantic'); unresolved = true;
      }
    }
    if (changes.length === 0) return clean;
    if (contract.mode === 'observe') return { ...clean, status: 'observed', driftClass, changes };
    if (unresolved) {
      const blocked = contract.mode === 'strict' || (contract.operation !== 'read' && (driftClass === 'semantic' || driftClass === 'unknown'));
      return { ...clean, status: blocked ? 'blocked' : 'degraded', driftClass, changes };
    }
    return { ...clean, status: 'repaired', driftClass, confidence, changes, byte_identical: false,
      value: Object.create(prototype, staged) as object };
  } catch { return { ...clean, status: 'bypassed' }; }
}
/** Same sequencing as the generated wrapper, without an upstream operation. */
export function simulateSandbox(contract: SandboxContract, args: unknown, recorded: unknown): Omit<SandboxResult, 'value'> & { readonly wouldWrite: boolean } {
  const request = evaluateSandbox(contract, args, 'request');
  const response = request.status === 'blocked' || request.status === 'bypassed' ? request : evaluateSandbox(contract, recorded, 'response');
  const phases = request === response ? [request] : [request, response];
  const status = phases.some(phase => phase.status === 'blocked') ? 'blocked'
    : phases.some(phase => phase.status === 'bypassed') ? 'bypassed'
    : phases.some(phase => phase.status === 'repaired') ? 'repaired'
    : phases.some(phase => phase.status === 'degraded') ? 'degraded'
    : phases.some(phase => phase.status === 'observed') ? 'observed' : 'ok';
  const scores = phases.flatMap(phase => phase.confidence === null ? [] : [phase.confidence]);
  return { status, confidence: scores.length === 0 ? null : Math.min(...scores),
    changes: phases.flatMap(phase => phase.changes), byte_identical: phases.every(phase => phase.byte_identical),
    driftClass: phases.some(phase => phase.driftClass === 'semantic') ? 'semantic' : phases.find(phase => phase.driftClass !== null)?.driftClass ?? null,
    wouldWrite: contract.operation !== 'read' };
}
