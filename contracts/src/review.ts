/** Shared TI-28 semantic bounds; no queue, network or guard decisions. */
import type { ReviewCase } from './generated/review_case.js';
import type { ReviewResolution } from './generated/review_resolution.js';
import { canonicalJson, copyBounded, freezeValue } from './bounds.js';

export const REVIEW_BYTES = 128 * 1024;
const REVIEW_BOUNDS = { bytes: REVIEW_BYTES, depth: 64, nodes: 50_000 };

export function reviewText(value: string, limit = 256): void {
  if (!value.trim() || [...value].length > limit || /\p{Cc}/u.test(value)) throw new Error('invalid review text');
}
export function copyReview(value: unknown): unknown {
  return copyBounded(value, REVIEW_BOUNDS);
}
export function freezeReview<T>(value: T): T {
  return freezeValue(value);
}
export function canonicalReview(value: unknown): string {
  return canonicalJson(value);
}
export function checkReviewCase(value: ReviewCase): void {
  reviewText(value.id);
  for (const id of Object.values(value.scope)) reviewText(id);
  if (value.id !== value.audit.id || value.created_at !== value.audit.ts
    || value.scope.contract_version !== value.audit.proof.contract_version) throw new Error('review audit identity mismatch');
  const phases = value.evidence;
  if (phases.length < 1 || phases.length > 2 || phases[0]?.side !== 'request'
    || (phases.length === 2 && phases[1]?.side !== 'response')
    || !phases.some(phase => phase.decision === 'halt')
    || value.dispatched !== (phases[0].decision !== 'halt')
    || (!value.dispatched && phases.length !== 1)) throw new Error('invalid review phase coverage');
  const changes = phases.flatMap(phase => phase.changes);
  const rank = { shape: 0, unknown: 1, semantic: 2 };
  const dominant = changes.reduce<ReviewCase['audit']['drift']['class'] | null>((prior, change) => prior === null || rank[change.class] > rank[prior] ? change.class : prior, null);
  const request = phases[0].changes.length > 0;
  const response = (phases[1]?.changes.length ?? 0) > 0;
  const side = request && response ? 'both' : request ? 'request' : 'response';
  const audited = value.audit.drift.changes;
  if (dominant !== value.audit.drift.class || side !== value.audit.drift.side
    || audited === undefined || audited.length !== changes.length
    || audited.some((change, index) => change.path !== changes[index]?.path || (change.to ?? null) !== changes[index]?.to)) throw new Error('incomplete/conflicting review evidence');
  for (const phase of phases) {
    if (phase.decision === 'halt' && phase.changes.length === 0) throw new Error('halt without drift evidence');
    if (phase.changes.length > 4096 || phase.proposals.length > 64) throw new Error('review evidence limit');
    for (const proposal of phase.proposals) {
      if (!/^\d+$/u.test(proposal.finding)
        || (proposal.replacement_hex !== null && (!/^(?:[0-9a-f]{2})*$/u.test(proposal.replacement_hex)))
        || (proposal.kind === 'rename' && (proposal.to === null || proposal.replacement_hex !== null))
        || (proposal.kind === 'replace' && (proposal.to !== null || proposal.replacement_hex === null))) throw new Error('invalid review proposal');
    }
  }
}
export function checkReviewResolution(value: ReviewResolution): void {
  checkReviewCase(value.case);
  if (value.revision === 16 && value.decision === 'defer') throw new Error('final revision must resolve review');
  reviewText(value.actor);
  reviewText(value.note, 4096);
  if (Date.parse(value.ts) < Date.parse(value.case.created_at)) throw new Error('resolution predates case');
  if (value.decision === 'accept_repair' && !value.case.evidence.some(phase => phase.proposals.length > 0)) throw new Error('no proposed repair');
}
