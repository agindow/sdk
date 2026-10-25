import type { Finding } from './generated/finding.js';
/** An investigated declaration remains separate from observed write correctness. */
export function checkFinding(value: Finding): void {
  const created = Date.parse(value.ts), expires = Date.parse(value.expires_at);
  if (!value.ts.endsWith('Z') || !value.expires_at.endsWith('Z') || expires <= created || expires - created > 30 * 86400000
    || !/^[a-f0-9]{64}$/u.test(value.fingerprint)) throw new Error('Invalid finding scope/lifetime');
  if (value.visibility === 'public' && (value.evidence_kind !== 'vendor' || value.hypothesis || value.citations.length === 0)) throw new Error('Private finding cannot be shared');
  if (['community', 'customer', 'missing', 'hypothesis'].includes(value.evidence_kind) && value.classification !== 'unknown') throw new Error('Awareness cannot classify');
  if (value.evidence_kind === 'hypothesis' && !value.hypothesis) throw new Error('Unlabeled hypothesis');
  checkFindingCitations(value.citations);
}
export function checkFindingCitations(citations: readonly {url:string;retrieved_at:string}[]):void {
  for (const citation of citations) {
    const url = new URL(citation.url);
    if (url.protocol !== 'https:' || url.username || url.password || url.hash || !citation.retrieved_at.endsWith('Z')) throw new Error('Invalid finding source');
    for (const key of url.searchParams.keys()) if (/token|secret|password|authorization|credential|key/iu.test(key)) throw new Error('Credential-bearing source');
  }
}
