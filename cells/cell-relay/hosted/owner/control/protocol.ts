import { hostPolicy, isHostPolicy, scopedHostPolicy } from './host-policy.js';
import { admissionPolicy, allowedRows } from './admission-policy.js';
import { digest, policy, record, type Registration } from '../policy.js';
export type Row = Omit<Registration, 'owner'>;
export type Status = 'active' | 'stopped' | 'revoked' | 'expired';
export interface Command {
  version: 1; origin: string; profile: string; generation: string; previousGeneration: string | null;
  expectedRevision: number; id: string; kid: string; iat: number; exp: number;
  action: 'admit' | 'resume' | 'stop' | 'revoke'; reviewUntil: number; digest: string; rows: readonly Row[];
}
export interface SignedCommand { payload: Command; signature: string }
export interface Snapshot {
  version: 1; review: SignedCommand; revision: number; sequence: number;
  issuedAt: number; notAfter: number; status: Status;
}
export interface LeaseState {
  status: Status; until: number; issuedAt: number; notAfter: number; sequence: number;
  digest: string; rows: readonly Row[];
}
export function exact(value: Record<string, unknown>, fields: readonly string[]): void {
  if (Object.keys(value).sort().join(',') !== [...fields].sort().join(',')) throw Error('invalid_fields');
}
export function integer(value: unknown, min = 0): number {
  if (typeof value !== 'number' || !Number.isSafeInteger(value) || value < min) throw Error('invalid_number');
  return value;
}
export function text(value: unknown, max = 256): string {
  if (typeof value !== 'string' || !value || value.length > max || /[\s\u007f]/u.test(value)) throw Error('invalid_text');
  return value;
}
/** Locale-independent Unicode code-point ordering; strings are never normalized. */
function compare(a: string, b: string): number {
  const left = Array.from(a, c => c.codePointAt(0) ?? 0), right = Array.from(b, c => c.codePointAt(0) ?? 0);
  for (let i = 0; i < Math.min(left.length, right.length); i++) {
    const x = left[i], y = right[i];
    if (x !== undefined && y !== undefined && x !== y) return x - y;
  }
  return left.length - right.length;
}
export function publicRow(row: Registration): Row {
  const { owner: _owner, ...fields } = row;
  return fields;
}
export async function reviewed(origin: string, profile: string, input: unknown): Promise<{ rows: readonly Row[]; digest: string }> {
  text(profile);
  const parsed = await policy(JSON.stringify({ issuer:origin, not_after:1, registrations:input }), 0);
  const rows = parsed.registrations.map(publicRow).map(row => ({...row, tools:[...row.tools].sort(compare)})).sort((a,b) => compare(a.id,b.id));
  const tuples = rows.map(r => [r.id,r.kid,r.x,r.audience,r.tenant,r.environment,r.connection,r.tools]);
  return { rows, digest:await digest(JSON.stringify(['agindow-keyless-review-v1',origin,profile,tuples])) };
}
export async function lease(state: LeaseState, now: number, origin: string, profile: string, interval = 600): Promise<{ issuedAt:number; notAfter:number; sequence:number }> {
  integer(now);
  if(!Number.isSafeInteger(interval) || interval<60 || interval>600) throw Error('interval');
  if (state.status !== 'active' || now >= state.notAfter || now >= state.until) throw Error('review_expired');
  if ((await reviewed(origin,profile,state.rows)).digest !== state.digest) throw Error('review_drift');
  if (now < state.issuedAt) throw Error('clock');
  // A duplicate scheduled delivery/retry does not manufacture another extension.
  if (now < state.issuedAt + interval) return { issuedAt:state.issuedAt, notAfter:state.notAfter, sequence:state.sequence };
  return { issuedAt:now, notAfter:Math.min(now+3600,state.until), sequence:integer(state.sequence+1,1) };
}
export function encoded(command: Command): string {
  return JSON.stringify(['agindow-review-command-v1',command.version,command.origin,command.profile,
    command.generation,command.previousGeneration,command.expectedRevision,command.id,command.kid,
    command.iat,command.exp,command.action,command.reviewUntil,command.digest]);
}
function bytes(value: string): Uint8Array<ArrayBuffer> {
  if (!/^[A-Za-z0-9_-]+$/u.test(value)) throw Error('signature');
  return Uint8Array.from(atob(value.replace(/-/gu,'+').replace(/_/gu,'/')), c=>c.charCodeAt(0));
}
export async function verified(raw: string, keysRaw: string, now: number, fresh: boolean): Promise<SignedCommand> {
  if (raw.length > 300000 || keysRaw.length > 4096) throw Error('capacity');
  const signed = record(JSON.parse(raw)); exact(signed,['payload','signature']);
  const c = record(signed.payload);
  exact(c,['version','origin','profile','generation','previousGeneration','expectedRevision','id','kid','iat','exp','action','reviewUntil','digest','rows']);
  if (c.version !== 1 || !['admit','resume','stop','revoke'].includes(String(c.action))) throw Error('command');
  const generation = text(c.generation), previousGeneration = c.previousGeneration === null ? null : text(c.previousGeneration);
  const iat = integer(c.iat), exp = integer(c.exp), reviewUntil = integer(c.reviewUntil);
  if (exp <= iat || exp > iat+300 || iat > now+30 || (fresh && exp <= now) || reviewUntil <= iat || reviewUntil > iat+2592000) throw Error('expiry');
  const origin = text(c.origin,2048), profile = text(c.profile);
  const content = await reviewed(origin,profile,c.rows);
  if (c.digest !== content.digest) throw Error('review_drift');
  const keys: unknown = JSON.parse(keysRaw);
  if (!Array.isArray(keys) || keys.length < 1 || keys.length > 2) throw Error('review_keys');
  const entries = keys.map(record);
  if (new Set(entries.map(k=>k.kid)).size !== entries.length) throw Error('review_keys');
  const key = entries.find(k=>k.kid===c.kid);
  if (!key) throw Error('review_key');
  exact(key,'policy' in key ? ['kid','x','until','policy'] : ['kid','x','until']);
  if('policy' in key){
    const p=isHostPolicy(key.policy)?await scopedHostPolicy(hostPolicy(key.policy),profile,content.rows,generation):admissionPolicy(key.policy);
    if(isHostPolicy(key.policy)&&hostPolicy(key.policy).reviewerKid!==c.kid)throw Error('admission_scope');
    if(p.origin!==origin||p.profile!==profile||generation!==p.bootstrapGeneration||reviewUntil>p.until)throw Error('admission_scope');
    allowedRows(p,content.rows);
  }
  if (integer(key.until) <= now || reviewUntil > integer(key.until)) throw Error('review_key');
  const command: Command = { version:1,origin,profile,generation,previousGeneration,
    expectedRevision:integer(c.expectedRevision),id:text(c.id),kid:text(c.kid),iat,exp,
    action:c.action as Command['action'], reviewUntil,digest:content.digest,rows:content.rows };
  const signature = text(signed.signature,128);
  const publicKey = await crypto.subtle.importKey('raw',bytes(text(key.x)),{name:'Ed25519'},false,['verify']);
  if (!await crypto.subtle.verify('Ed25519',publicKey,bytes(signature),new TextEncoder().encode(encoded(command)))) throw Error('signature');
  return { payload:command, signature };
}
