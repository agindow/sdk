import { operationComparison } from './wasm.js';
export interface OperationScope {
  readonly route: string; readonly method: 'GET' | 'POST' | 'PUT' | 'PATCH' | 'DELETE' | 'HEAD' | 'OPTIONS';
  readonly side: 'request' | 'response'; readonly version: string;
}
export interface OperationSource {
  readonly scope: OperationScope; readonly kind: 'declared' | 'observed';
  readonly provenance: string; readonly body: string; readonly complete: boolean;
}
export interface OperationFinding {
  /** Native escaped path. [] denotes a declared/observed item shape, never a selected repair target. */
  readonly path: string; readonly pathKind: 'schema' | 'concrete'; readonly kind: string; readonly class: 'shape' | 'semantic' | 'unknown';
  readonly compatibility: 'compatible' | 'breaking' | 'unknown';
  readonly provenance: {readonly baseline: string; readonly current: string; readonly kind: 'declared' | 'observed'};
}
export interface OperationComparisonResult {
  readonly coverage: 'complete' | 'partial' | 'unsupported'; readonly gaps: readonly string[];
  readonly requiredness: 'declared' | 'unknown'; readonly findings: readonly OperationFinding[];
}
export interface OperationComparisonInput { readonly baseline: OperationSource; readonly current: OperationSource }
/** Explicit advisory comparison using the packaged guest. No baseline cache or policy mutation. */
export async function compareOperation(input: OperationComparisonInput, work = 200_000_000): Promise<OperationComparisonResult> {
  try {
    if (!Number.isInteger(work) || work < 0 || work > 0xffff_ffff) throw Error('invalid comparison budget');
    const serialized = JSON.stringify(input);
    if (Buffer.byteLength(serialized) > 2_200_000) throw Error('comparison capture limit');
    const value: unknown = JSON.parse(await operationComparison(serialized, work));
    if (typeof value !== 'object' || value === null || !('coverage' in value) || !['complete','partial','unsupported'].includes(String(value.coverage))
      || !('gaps' in value) || !Array.isArray(value.gaps) || !value.gaps.every((gap: unknown) => typeof gap === 'string')
      || !('requiredness' in value) || !['declared','unknown'].includes(String(value.requiredness))
      || !('findings' in value) || !Array.isArray(value.findings)) throw Error('invalid operation result');
    return value as unknown as OperationComparisonResult;
  } catch { return {coverage: 'unsupported', gaps: ['comparison-unavailable'], requiredness: 'unknown', findings: []}; }
}
