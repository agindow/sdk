import type { DeliveryRetry } from './delivery-plan.js';
import { integer, reviewed, type SignedCommand, type Snapshot } from './protocol.js';
export interface ControlState {
  generation: string; revision: number; floor: number; snapshot: Snapshot | null; pending: string[]; lastFailure:string|null; retry:DeliveryRetry[];
}
export function initial(generation: string, now: number): ControlState {
  return {generation,revision:0,floor:now,snapshot:null,pending:[],retry:[],lastFailure:null};
}
export async function transition(state: ControlState, signed: SignedCommand, now: number): Promise<ControlState> {
  const c = signed.payload, old = state.snapshot;
  if (now < state.floor) throw Error('clock');
  if (c.generation !== state.generation || c.expectedRevision !== state.revision) throw Error('review_conflict');
  if (c.reviewUntil <= now) throw Error('review_expired');
  if ((await reviewed(c.origin,c.profile,c.rows)).digest !== c.digest) throw Error('review_drift');
  if (c.action !== 'admit' && (!old || c.digest !== old.review.payload.digest)) throw Error('review_conflict');
  if (c.action === 'resume' && old?.status === 'revoked') throw Error('review_revoked');
  const targets = [...new Set([...(old?.review.payload.rows.map(r=>r.id) ?? []),...c.rows.map(r=>r.id)])];
  if (targets.length > 256) throw Error('capacity');
  const status = c.action==='stop' ? 'stopped' : c.action==='revoke' ? 'revoked' : 'active';
  return {generation:state.generation,revision:integer(state.revision+1,1),floor:now,pending:targets,retry:[],lastFailure:null,
    snapshot:{version:1,review:signed,revision:state.revision+1,sequence:integer((old?.sequence ?? 0)+1,1),
      issuedAt:now,notAfter:Math.min(now+3600,c.reviewUntil),status}};
}
export function health(state: ControlState): string {
  return JSON.stringify({generation:state.generation,revision:state.revision,
    sequence:state.snapshot?.sequence ?? 0,status:state.snapshot?.status ?? 'unadmitted',
    notAfter:state.snapshot?.notAfter ?? 0,reviewUntil:state.snapshot?.review.payload.reviewUntil ?? 0,pending:state.pending.length,lastFailure:state.lastFailure});
}

export function reason(error: unknown): string {
  const message = error instanceof Error ? error.message : '';
  return ['review_drift','review_expired','review_inactive','review_required','review_key','clock','capacity'].includes(message) ? message : 'authority_unavailable';
}
