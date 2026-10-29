import type { BindingSelection, CheckoutGrant, SourceBinding } from './types.js';
export function record(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}
export function isGrant(value: unknown): value is CheckoutGrant {
  return record(value) && ['root', 'tenantId', 'repositoryId', 'baseSha', 'actorId', 'authorityRevision'].every(key => typeof value[key] === 'string');
}
export function isSelection(value: unknown): value is BindingSelection {
  return record(value) && ['tenantId', 'repositoryId', 'operation', 'file', 'symbol', 'field'].every(key => typeof value[key] === 'string')
    && (value['side'] === 'request' || value['side'] === 'response') && typeof value['bindingVersion'] === 'number';
}
export function isBinding(value: unknown): value is SourceBinding {
  if (!record(value)) return false;
  const span = value['span'];
  return isSelection(value) && value['formatVersion'] === 1
    && ['baseSha', 'actorId', 'authorityRevision', 'fileDigest', 'bindingId'].every(key => typeof value[key] === 'string')
    && record(span) && ['start', 'end', 'line', 'column'].every(key => Number.isSafeInteger(span[key]));
}
