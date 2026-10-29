import { isBinding } from './input.js';
import { digest, readSource } from './checkout.js';
import { resolveSpan } from './resolve.js';
import { abstain } from './types.js';
import type { BindingResult, BindingSelection, CheckoutGrant, SourceBinding } from './types.js';
export type { BindingResult, BindingSelection, CheckoutGrant, SourceBinding } from './types.js';
function identifier(value: unknown): value is string {
  return typeof value === 'string' && value.length > 0 && value.length <= 256 && !/[\x00-\x1f\x7f]/.test(value);
}
export function bindSource(grant: CheckoutGrant, selection: BindingSelection): BindingResult {
  if (!identifier(grant.tenantId) || !identifier(grant.repositoryId) || !identifier(grant.actorId) || !identifier(grant.authorityRevision)
    || typeof grant.baseSha !== 'string' || !/^(?:[a-f0-9]{40}|[a-f0-9]{64})$/.test(grant.baseSha)
    || typeof grant.root !== 'string' || typeof selection.file !== 'string'
    || !identifier(selection.operation) || !identifier(selection.symbol) || !identifier(selection.field)
    || !Number.isSafeInteger(selection.bindingVersion) || selection.bindingVersion < 1
    || !['request', 'response'].includes(selection.side)) return abstain('invalid_selection');
  if (grant.tenantId !== selection.tenantId || grant.repositoryId !== selection.repositoryId) return abstain('unauthorized');
  const read = readSource(grant, selection.file);
  if ('reason' in read) return abstain(read.reason);
  const found = resolveSpan(read.source, selection);
  if ('status' in found) return found;
  const identity = {
    formatVersion: 1 as const, tenantId: selection.tenantId, repositoryId: selection.repositoryId,
    operation: selection.operation, side: selection.side, file: selection.file, symbol: selection.symbol, field: selection.field,
    bindingVersion: selection.bindingVersion, baseSha: grant.baseSha, actorId: grant.actorId, authorityRevision: grant.authorityRevision,
    fileDigest: read.fileDigest, span: found,
  };
  return { status: 'bound', manifest: { ...identity, bindingId: digest(JSON.stringify(identity)) } };
}
/** Rebuild from current admitted authority and bytes; never trust stored spans. */
export function resolveBinding(grant: CheckoutGrant, manifest: unknown): BindingResult {
  if (!isBinding(manifest)) return abstain('binding_mismatch');
  const result = bindSource(grant, manifest);
  if (result.status !== 'bound') return result;
  const expected = result.manifest;
  if (Object.keys(manifest).length !== Object.keys(expected).length
    || Object.entries(expected).some(([key, value]) => JSON.stringify(manifest[key as keyof SourceBinding]) !== JSON.stringify(value))) return abstain('binding_mismatch', [expected.span]);
  return result;
}
