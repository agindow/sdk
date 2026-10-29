import type { SourceRepairAuthority, SourceRepairEvidence, SourceRepairRefusal } from 'contracts';
import type { CheckoutGrant, SourceBinding, RefusalReason } from '../binding/types.js';
/** Admitted from current owner policy by the host, separately from publication. */
export interface GenerationGrant {
  readonly authority: SourceRepairAuthority;
  readonly evidenceDigest: string;
  readonly bindingId: string;
  readonly factId: string;
  readonly repositoryId: string;
  readonly actorId: string;
  readonly authorityRevision: string;
}
export interface MigrationInput {
  readonly checkout: CheckoutGrant;
  readonly binding: SourceBinding;
  readonly evidence: SourceRepairEvidence;
  readonly grant: GenerationGrant;
}
export interface MigrationReceipt {
  readonly formatVersion: 1;
  readonly recipe: 'js-ts-top-level-rename';
  readonly recipeVersion: 1;
  readonly validation: 'unvalidated';
  readonly evaluatedAt: string;
  readonly tenantId: string;
  readonly repositoryId: string;
  readonly actorId: string;
  readonly authorityRevision: string;
  readonly baseSha: string;
  readonly bindingId: string;
  readonly factId: string;
  readonly evidenceDigest: string;
  readonly factDigest: string;
  readonly originalDigest: string;
  readonly baseDigest: string;
  readonly resultDigest: string;
  readonly patchDigest: string;
}
export type MigrationRefusal = RefusalReason | SourceRepairRefusal | 'unauthorized_generation' | 'unsafe_field' | 'collision' | 'patch_limit';
export type MigrationResult = { readonly status: 'generated'; readonly patch: string; readonly receipt: MigrationReceipt; readonly receiptDigest: string }
  | { readonly status: 'noop'; readonly reason: 'already_present'; readonly receipt: MigrationReceipt; readonly receiptDigest: string }
  | { readonly status: 'refused'; readonly reason: MigrationRefusal };
export const refused = (reason: MigrationRefusal): MigrationResult => ({ status: 'refused', reason });
