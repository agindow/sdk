import { canonicalJson, copyBounded, freezeValue, validateSourceRepairEvidence } from 'contracts';
import { isBinding, isGrant, record } from '../binding/input.js';
import { digest, MAX_SOURCE_BYTES, readSource } from '../binding/checkout.js';
import { resolveBinding } from '../binding/index.js';
import { resolveSpan } from '../binding/resolve.js';
import { isGenerationGrant } from './admission.js';
import { unifiedPatch } from './patch.js';
import { refused } from './types.js';
import type { MigrationReceipt, MigrationResult } from './types.js';
export type { MigrationInput, MigrationResult, GenerationGrant, MigrationReceipt } from './types.js';
const dangerous = new Set(['__proto__', 'prototype', 'constructor']);
/** Off-path local export only. Fresh trusted host admission is required; a digest grants nothing. */
export async function generateSourcePatch(value: unknown, now: number): Promise<MigrationResult> {
  try {
    // Detach all caller inputs before the evidence validator's asynchronous digest.
    const input = freezeValue(copyBounded(value, { bytes: 64 * 1024, nodes: 4096, depth: 12 }));
    if (!record(input) || !isGrant(input['checkout']) || !isBinding(input['binding']) || !isGenerationGrant(input['grant'])) return refused('invalid_selection');
    const checkout = input['checkout'], binding = input['binding'], grant = input['grant'];
    const admitted = await validateSourceRepairEvidence(input['evidence'], grant.authority, now);
    if (!admitted.ok) return refused(admitted.reason);
    const evidence = admitted.evidence;
    if (grant.evidenceDigest !== evidence.digest || grant.bindingId !== binding.bindingId || grant.repositoryId !== binding.repositoryId
      || grant.actorId !== checkout.actorId || grant.repositoryId !== checkout.repositoryId) return refused('unauthorized_generation');
    if (evidence.scope.tenant !== binding.tenantId || evidence.scope.tenant !== checkout.tenantId
      || evidence.scope.operation !== binding.operation || evidence.scope.side !== binding.side) return refused('scope_mismatch');
    const fact = evidence.facts.find(item => item.id === grant.factId);
    if (fact === undefined) return refused('missing_facts');
    const from = fact.from.slice(1), to = fact.to.slice(1);
    if (binding.field !== from && binding.field !== to) return refused('scope_mismatch');
    if (dangerous.has(from) || dangerous.has(to)) return refused('unsafe_field');
    const resolved = resolveBinding(checkout, binding);
    if (resolved.status !== 'bound') return refused(resolved.reason);
    const read = readSource(checkout, binding.file);
    if ('reason' in read) return refused(read.reason);
    if (read.fileDigest !== binding.fileDigest) return refused('stale_source');
    const source = read.source;
    if (binding.field === to) {
      const old = resolveSpan(source, { ...binding, field: from });
      if (!('status' in old) || (old.status === 'abstained' && old.reason === 'ambiguous')) return refused('collision');
    }
    let changed = source, patch = '';
    if (binding.field === from) {
      const token = source.slice(binding.span.start, binding.span.end);
      const quote = token[0] === "'" || token[0] === '"' ? token[0] : '';
      const replacement = quote + to + quote;
      changed = source.slice(0, binding.span.start) + replacement + source.slice(binding.span.end);
      if (Buffer.byteLength(changed) > MAX_SOURCE_BYTES) return refused('source_limit');
      // Independent AST resolution of the new key detects target collisions and unsupported output.
      const target = resolveSpan(changed, { ...binding, field: to });
      if ('status' in target) return refused(target.status === 'abstained' && target.reason === 'ambiguous' ? 'collision' : 'unsupported_syntax');
      patch = unifiedPatch(binding.file, source, changed);
      if (Buffer.byteLength(patch) > 2 * MAX_SOURCE_BYTES + 16384) return refused('patch_limit');
    }
    const receipt: MigrationReceipt = {
      formatVersion: 1, recipe: 'js-ts-top-level-rename', recipeVersion: 1, validation: 'unvalidated',
      evaluatedAt: new Date(now).toISOString(), tenantId: binding.tenantId, repositoryId: binding.repositoryId, actorId: grant.actorId, authorityRevision: grant.authorityRevision,
      baseSha: checkout.baseSha, bindingId: binding.bindingId, factId: fact.id, evidenceDigest: evidence.digest,
      factDigest: digest(canonicalJson(fact)), originalDigest: read.fileDigest, baseDigest: read.fileDigest,
      resultDigest: digest(changed), patchDigest: digest(patch),
    };
    const receiptDigest = digest(canonicalJson(receipt));
    return freezeValue(binding.field === to ? { status: 'noop', reason: 'already_present', receipt, receiptDigest }
      : { status: 'generated', patch, receipt, receiptDigest });
  } catch { return refused('invalid_selection'); }
}
