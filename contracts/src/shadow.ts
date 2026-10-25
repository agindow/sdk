/** TII-11 shadow evidence semantics; never fabricates a strict blocked audit. */
import type { ShadowReview } from './generated/shadow_review.js';
import type { ReviewPhase } from './generated/review_case.js';
import { reviewText, canonicalReview } from './review.js';
export function reviewPlanText(phases: readonly ReviewPhase[]): string {
  return canonicalReview(phases.map(phase => ({ side: phase.side, proposals: phase.proposals.map(proposal => {
    const finding = phase.changes[Number(proposal.finding)];
    if (!/^\d+$/u.test(proposal.finding) || finding === undefined) throw new Error('Proposal has no finding');
    return { path: finding.path, kind: proposal.kind, to: proposal.to, replacement_hex: proposal.replacement_hex };
  }) })));
}
export function checkShadowReview(value: ShadowReview): void {
  const c = value.case;
  reviewText(c.id); for (const id of Object.values(c.scope)) reviewText(id);
  if (c.id !== c.audit.id || c.created_at !== c.audit.ts || c.scope.contract_version !== c.audit.proof.contract_version
    || c.audit.proof.write_integrity || c.audit.patch?.applied
    || (c.audit.patch !== null && !c.audit.patch.byte_identical)
    || c.dispatched === (c.observed_outcome === 'not_dispatched')) throw new Error('Invalid shadow audit/outcome');
  if (c.evidence.length < 1 || c.evidence.length > 2 || c.evidence[0]?.side !== 'request'
    || (c.evidence.length === 2 && (c.evidence[1]?.side !== 'response' || !c.dispatched))) throw new Error('Invalid shadow phase coverage');
  const changes = c.evidence.flatMap(p => p.changes), audited = c.audit.drift.changes;
  if (changes.length === 0 || audited === undefined || changes.length !== audited.length
    || changes.some((change, i) => change.path !== audited[i]?.path || change.to !== (audited[i]?.to ?? null))) throw new Error('Incomplete shadow drift evidence');
  const rank = { shape: 0, unknown: 1, semantic: 2 };
  const dominant = changes.reduce<'shape' | 'semantic' | 'unknown'>((prior, change) => rank[change.class] > rank[prior] ? change.class : prior, 'shape');
  const request = (c.evidence[0]?.changes.length ?? 0) > 0, response = (c.evidence[1]?.changes.length ?? 0) > 0;
  if (c.audit.drift.class !== dominant || c.audit.drift.side !== (request && response ? 'both' : request ? 'request' : 'response')) throw new Error('Conflicting shadow classification');
  for (const phase of c.evidence) {
    if (phase.changes.length > 4096 || phase.proposals.length > 64) throw new Error('Shadow evidence limit');
    for (const p of phase.proposals) {
      if ((p.kind === 'rename' && (p.to === null || p.replacement_hex !== null))
        || (p.kind === 'replace' && (p.to !== null || p.replacement_hex === null || !/^(?:[a-f0-9]{2})*$/u.test(p.replacement_hex)))) throw new Error('Invalid shadow proposal');
    }
  }
  reviewPlanText(c.evidence);
  const r = value.resolution;
  if (r !== null) {
    reviewText(r.actor); reviewText(r.note, 4096);
    if (Date.parse(r.ts) < Date.parse(c.created_at) || (r.revision === 16 && r.decision === 'defer')
      || (r.decision === 'accept_repair' && !c.evidence.some(p => p.proposals.length > 0))) throw new Error('Invalid shadow resolution');
  }
}
