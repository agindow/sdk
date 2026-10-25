import type { InvestigationJob } from './generated/investigation_job.js';

export const INVESTIGATION_BYTES = 4096;
export const INVESTIGATION_TTL_MS = 600_000;
export function microUnits(value: string): bigint {
  if (!/^(0|[1-9][0-9]{0,19})$/u.test(value) || BigInt(value) > 18446744073709551615n) throw new Error('Invalid micro-units');
  return BigInt(value);
}
export function checkInvestigation(job: InvestigationJob): void {
  microUnits(job.ceiling);
  const created = Date.parse(job.created_at), expires = Date.parse(job.expires_at);
  if (!job.created_at.endsWith('Z') || !job.expires_at.endsWith('Z') || expires <= created || expires - created > INVESTIGATION_TTL_MS) {
    throw new Error('Invalid investigation lifetime');
  }
  for (const text of [job.tenant, job.id, job.audit_id, job.call_id, job.price_context]) {
    if (!/^[A-Za-z0-9_.:-]{1,256}$/u.test(text)) throw new Error('Invalid investigation identifier');
  }
}
