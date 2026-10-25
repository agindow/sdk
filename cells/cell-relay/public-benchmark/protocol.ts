import { createPublicKey, verify } from 'node:crypto';
import { canonical, hash, validateCorpus } from './corpus.js';
import { baselines, validateReport } from './scorer.js';
import type { Attestation, Commitment, Corpus, Report, Validator } from './types.js';

export function commitHeldout(bytes: string, publicCorpus: Corpus): Commitment {
  validateCorpus(publicCorpus);
  const sealed: unknown = JSON.parse(bytes);
  validateCorpus(sealed);
  const publicIds = new Set(publicCorpus.cases.map(c => c.id));
  const publicPayloads = new Set(publicCorpus.cases.map(c => canonical(c.payload)));
  if (publicCorpus.split !== 'development' || sealed.split !== 'heldout' || sealed.version === publicCorpus.version
    || sealed.cases.some(c => publicIds.has(c.id) || publicPayloads.has(canonical(c.payload)))) throw new Error('Held-out split must be private and disjoint');
  return { version: sealed.version, sha256: hash(bytes) };
}
export function validateCommitment(value: unknown): asserts value is Commitment {
  if (value === null || typeof value !== 'object' || !('version' in value) || !('sha256' in value)
    || typeof value.version !== 'string' || !value.version || typeof value.sha256 !== 'string'
    || !/^[0-9a-f]{64}$/.test(value.sha256) || Object.keys(value).sort().join(',') !== 'sha256,version') throw new Error('A sealed commitment is required');
}
export function verifyHeldout(bytes: string, commitment: Commitment, publicCorpus: Corpus): void {
  validateCommitment(commitment);
  if (canonical(commitHeldout(bytes, publicCorpus)) !== canonical(commitment)) throw new Error('Sealed commitment mismatch');
}
export function releaseStatement(corpus: Corpus, reports: Report[], commitment: Commitment, artifactDigest: string): string {
  validateCommitment(commitment);
  if (!/^[0-9a-f]{64}$/.test(artifactDigest)) throw new Error('Pin the public artifact digest');
  const expected = baselines(corpus);
  if (corpus.split !== 'development' || reports.length !== 2) throw new Error('Launch requires both development baselines');
  for (const report of reports) validateReport(report, corpus);
  const sorted = [...reports].sort((a, b) => a.entry.localeCompare(b.entry, 'en'));
  if (canonical(sorted) !== canonical(expected.sort((a, b) => a.entry.localeCompare(b.entry, 'en')))) throw new Error('Launch permits only reproducible no-repair and naive-full-payload baselines');
  return canonical({ protocol: 'tii-21a-release-v1', artifactDigest, commitment,
    reports: sorted.map(report => ({ entry: report.entry, sha256: hash(canonical(report)) })) });
}
// Trust roots are supplied out of band by the publication owner, never by a submission.
// Cryptography proves attribution; the owner must verify external affiliation independently.
export function release(corpus: Corpus, reports: Report[], commitment: Commitment | undefined, attestations: Attestation[], artifactDigest: string, validators: Validator[]): object {
  validateCommitment(commitment);
  const statement = releaseStatement(corpus, reports, commitment, artifactDigest);
  if (attestations.length < 2 || attestations.length > 3 || new Set(attestations.map(a => a.validator)).size !== attestations.length) throw new Error('Two or three distinct external validators must co-sign');
  const keys = new Set<string>();
  for (const a of attestations) {
    const matches = validators.filter(v => v.id === a.validator);
    const validator = matches[0];
    if (matches.length !== 1 || !validator) throw new Error('Untrusted validator');
    const key = createPublicKey(validator.publicKey);
    const fingerprint = hash(key.export({ type: 'spki', format: 'pem' }).toString());
    if (key.asymmetricKeyType !== 'ed25519' || keys.has(fingerprint)
      || !verify(null, Buffer.from(statement), key, Buffer.from(a.signature, 'base64'))) throw new Error('Untrusted or invalid external signature');
    keys.add(fingerprint);
  }
  return { statement: JSON.parse(statement) as unknown, attestations, reports };
}
