import type { SourceRepairEvidence, Scope } from './generated/source_repair_evidence.js';
import { copyBounded, freezeValue, canonicalJson } from './bounds.js';
import { validatorFor } from './compile.js';
import { suspiciousContent } from './notification.js';

export const SOURCE_REPAIR_BYTES = 32 * 1024;
export const SOURCE_REPAIR_TTL_MS = 7 * 86400000;
export type SourceRepairAuthority = Scope & { finding_id: string; finding_revision: number };
export type SourceRepairRefusal = 'invalid_evidence' | 'resource_limit' | 'scope_mismatch' | 'missing_facts'
  | 'hypothesis_only' | 'expired' | 'contradictory_facts' | 'consent_required' | 'unsafe_content' | 'digest_mismatch';
export type SourceRepairResult = { ok: true; evidence: SourceRepairEvidence } | { ok: false; reason: SourceRepairRefusal };

/** No caller content is echoed by refusal results. */
export class SourceRepairError extends Error {
  readonly reason: SourceRepairRefusal;
  constructor(reason: SourceRepairRefusal) { super(reason); this.reason = reason; }
}
export function refuse(reason: SourceRepairRefusal): never { throw new SourceRepairError(reason); }
export function sourceRepairFailure(error: unknown): SourceRepairResult {
  return { ok: false, reason: error instanceof SourceRepairError ? error.reason : 'invalid_evidence' };
}
export function boundedSourceRepair(value: unknown): unknown {
  try { return copyBounded(value, { bytes: SOURCE_REPAIR_BYTES, nodes: 2048, depth: 10 }); }
  catch { return refuse('resource_limit'); }
}

/** Structural admission detaches before hashing/awaiting; consumers still need scope/time/digest validation. */
export function parseSourceRepairEvidence(value: unknown): SourceRepairEvidence {
  const copied = boundedSourceRepair(value);
  const validate = validatorFor('source_repair_evidence');
  if (!validate(copied)) {
    if (validate.errors?.some(error => /^\/facts\/\d+\/scope/u.test(error.instancePath))) refuse('scope_mismatch');
    if (validate.errors?.some(error => error.instancePath.startsWith('/attachments'))) refuse('consent_required');
    refuse('invalid_evidence');
  }
  // The single cast at the schema boundary.
  return freezeValue(copied as SourceRepairEvidence);
}

export function sameSourceRepairScope(a: Scope, b: Scope): boolean {
  return a.tenant === b.tenant && a.connection === b.connection && a.operation === b.operation
    && a.side === b.side && a.contract_revision === b.contract_revision && a.evidence_revision === b.evidence_revision;
}
function safeText(text: string): void {
  // Hex digests are scanned separately by the schema, never as payload text.
  if (suspiciousContent(text)) refuse('unsafe_content');
}
function at(text: string): number {
  const value = Date.parse(text);
  if (!text.endsWith('Z') || !Number.isFinite(value)) refuse('invalid_evidence');
  return value;
}
function interval(start: string, end: string, now: number): void {
  if (at(end) <= now) refuse('expired');
  if (at(start) > now || at(end) <= at(start)) refuse('invalid_evidence');
}

/** Reviewed references are trusted host inputs, not self-authenticating grants. */
export function checkSourceRepairEvidence(evidence: SourceRepairEvidence, authority: SourceRepairAuthority, now: number): void {
  if (!Number.isFinite(now)) refuse('invalid_evidence');
  if (!sameSourceRepairScope(evidence.scope, authority) || evidence.finding.id !== authority.finding_id
    || evidence.finding.revision !== authority.finding_revision) refuse('scope_mismatch');
  for (const value of Object.values(evidence.scope)) safeText(value);
  for (const value of [evidence.finding.id, evidence.finding.investigation_id, evidence.finding.field]) safeText(value);
  interval(evidence.created_at, evidence.expires_at, now);
  interval(evidence.finding.observed_at, evidence.finding.expires_at, now);
  if (at(evidence.expires_at) - at(evidence.created_at) > SOURCE_REPAIR_TTL_MS
    || at(evidence.expires_at) > at(evidence.finding.expires_at)
    || at(evidence.finding.observed_at) > at(evidence.created_at)) refuse('invalid_evidence');
  const ids = new Set<string>(), fields = new Set<string>();
  for (const fact of evidence.facts) {
    if (!sameSourceRepairScope(fact.scope, evidence.scope)) refuse('scope_mismatch');
    interval(fact.reviewed_at, fact.expires_at, now);
    if (at(fact.reviewed_at) > at(evidence.created_at) || at(evidence.expires_at) > at(fact.expires_at)) refuse('invalid_evidence');
    for (const text of [fact.id, fact.from, fact.to, fact.reviewed_by, fact.provenance.reference, fact.provenance.revision]) safeText(text);
    if (ids.has(fact.id) || fields.has(fact.from) || fields.has(fact.to) || fact.from === fact.to) refuse('contradictory_facts');
    ids.add(fact.id); fields.add(fact.from); fields.add(fact.to);
  }
  // The incident field must be covered by the reviewed declarations.
  if (!evidence.facts.some(fact => fact.from === evidence.finding.field)) refuse('missing_facts');
  const attachments = new Set<string>();
  for (const attachment of evidence.attachments) {
    const consent = attachment.consent;
    if (!sameSourceRepairScope(consent.scope, evidence.scope) || consent.kind !== attachment.kind
      || consent.evidence_digest !== attachment.digest) refuse('consent_required');
    interval(consent.granted_at, consent.expires_at, now);
    if (at(consent.granted_at) > at(evidence.created_at) || at(evidence.expires_at) > at(consent.expires_at)) refuse('consent_required');
    for (const text of [attachment.reference, consent.id, consent.actor]) safeText(text);
    const key = `${attachment.kind}:${attachment.reference}`;
    if (attachments.has(key)) refuse('invalid_evidence');
    attachments.add(key);
  }
}

/** SHA-256 over canonical JSON excluding only the envelope digest. Not a signature. */
export async function sourceRepairDigest(evidence: SourceRepairEvidence): Promise<string> {
  const { digest: _digest, ...body } = evidence;
  const hash = await globalThis.crypto.subtle.digest('SHA-256', new TextEncoder().encode(canonicalJson(body)));
  return Array.from(new Uint8Array(hash), byte => byte.toString(16).padStart(2, '0')).join('');
}

/** Pure consumer boundary: call before resolving any source or provider references. */
export async function validateSourceRepairEvidence(value: unknown, authority: SourceRepairAuthority, now: number): Promise<SourceRepairResult> {
  try {
    const evidence = parseSourceRepairEvidence(value);
    checkSourceRepairEvidence(evidence, authority, now);
    if (await sourceRepairDigest(evidence) !== evidence.digest) refuse('digest_mismatch');
    return { ok: true, evidence };
  } catch (error) { return sourceRepairFailure(error); }
}
