import { record } from '../binding/input.js';
import type { GenerationGrant } from './types.js';
function text(value: unknown): value is string {
  return typeof value === 'string' && value.length > 0 && value.length <= 256 && !/[\x00-\x1f\x7f]/.test(value);
}
export function isGenerationGrant(value: unknown): value is GenerationGrant {
  if (!record(value)) return false;
  const authority = value['authority'];
  return ['evidenceDigest', 'bindingId', 'factId', 'repositoryId', 'actorId', 'authorityRevision'].every(key => text(value[key]))
    && record(authority) && ['tenant', 'connection', 'operation', 'contract_revision', 'evidence_revision', 'finding_id'].every(key => text(authority[key]))
    && (authority['side'] === 'request' || authority['side'] === 'response') && Number.isSafeInteger(authority['finding_revision']);
}
