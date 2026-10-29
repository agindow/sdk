/** Host-admitted local authority. Never infer this from inventory or a manifest. */
export interface CheckoutGrant {
  readonly root: string;
  readonly tenantId: string;
  readonly repositoryId: string;
  readonly baseSha: string;
  readonly actorId: string;
  readonly authorityRevision: string;
}
export interface BindingSelection {
  readonly tenantId: string;
  readonly repositoryId: string;
  readonly operation: string;
  readonly side: 'request' | 'response';
  readonly file: string;
  readonly symbol: string;
  readonly field: string;
  readonly bindingVersion: number;
}
/** Offsets are UTF-16 source offsets, end exclusive; line/column are one based. */
export interface SourceSpan { readonly start: number; readonly end: number; readonly line: number; readonly column: number }
export interface SourceBinding extends BindingSelection {
  readonly formatVersion: 1;
  readonly baseSha: string;
  readonly actorId: string;
  readonly authorityRevision: string;
  readonly fileDigest: string;
  readonly span: SourceSpan;
  readonly bindingId: string;
}
export type RefusalReason = 'invalid_selection' | 'unauthorized' | 'unsafe_path' | 'excluded_source'
  | 'source_limit' | 'stale_base' | 'stale_source' | 'unsupported_syntax' | 'ambiguous' | 'binding_mismatch' | 'unavailable';
export type BindingResult = { readonly status: 'bound'; readonly manifest: SourceBinding }
  | { readonly status: 'abstained'; readonly reason: RefusalReason; readonly locations: readonly SourceSpan[]; readonly manual: string };
export function abstain(reason: RefusalReason, locations: readonly SourceSpan[] = [{ start: 0, end: 0, line: 1, column: 1 }]): BindingResult {
  return { status: 'abstained', reason, locations,
    manual: 'Select the tenant-authorized repository, current commit, file and unique top-level symbol explicitly; review a direct serializer/access or refactor unsupported syntax, then bind again. No hint or manual offset authorizes repair.' };
}
