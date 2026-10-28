import { hostPolicy, scopedHostPolicy } from './host-policy.js';
import type { DurableObjectStorage } from '@cloudflare/workers-types';
import { digest, record, type Registration } from '../policy.js';
import { exact, integer, verified, type Snapshot, type Status } from './protocol.js';
export interface Projection {
  generation:string; revision:number; sequence:number; issuedAt:number; notAfter:number;
  status:Status; row:Registration|null; digest:string; origin:string;
  history:readonly {row:Registration;until:number}[];
}
export async function accept(current: Projection | null, input: unknown, registration: string, origin: string,
  profile: string, keys: string, now: number, hostRaw?:string): Promise<Projection> {
  const value = record(input);
  exact(value,['version','review','revision','sequence','issuedAt','notAfter','status']);
  if (value.version!==1 || !['active','stopped','revoked','expired'].includes(String(value.status))) throw Error('snapshot');
  const signed = await verified(JSON.stringify(value.review),keys,now,false), c = signed.payload;
  if(hostRaw!==undefined)await scopedHostPolicy(hostPolicy(JSON.parse(hostRaw)),profile,c.rows,c.generation);
  const revision = integer(value.revision,1), sequence = integer(value.sequence,1), issuedAt = integer(value.issuedAt), notAfter = integer(value.notAfter);
  if (c.origin!==origin || c.profile!==profile || revision!==c.expectedRevision+1 || issuedAt>now+30 ||
    notAfter<=issuedAt || notAfter>issuedAt+3600 || notAfter>c.reviewUntil ||
    (value.status==='active' && (notAfter<=now || !['admit','resume'].includes(c.action)))) throw Error('snapshot');
  if (current) {
    if (now<current.issuedAt) throw Error('clock');
    if (c.generation!==current.generation) {
      if (c.action!=='admit' || c.previousGeneration!==current.generation || c.exp<=now) throw Error('generation');
    } else {
      if (revision<current.revision || sequence<current.sequence) throw Error('stale_snapshot');
      if (sequence===current.sequence) {
        if (revision!==current.revision || c.digest!==current.digest || notAfter!==current.notAfter || value.status!==current.status) throw Error('snapshot_conflict');
        return current;
      }
      if (revision===current.revision && (c.digest!==current.digest || notAfter<current.notAfter)) throw Error('snapshot_conflict');
    }
  }
  const raw = c.rows.find(r=>r.id===registration);
  const row = raw ? {...raw,owner:await digest('agindow-owner-v1:'+raw.id)} : null;
  const history = (current?.history ?? []).filter(h=>h.until>now);
  if (current?.row && JSON.stringify(current.row)!==JSON.stringify(row)) history.push({row:current.row,until:now+330});
  if (history.length>16) throw Error('capacity');
  return {generation:c.generation,revision,sequence,issuedAt,notAfter,status:value.status as Snapshot['status'],
    row,digest:c.digest,origin,history};
}
export function loadProjection(storage: DurableObjectStorage): Projection | null {
  storage.sql.exec('CREATE TABLE IF NOT EXISTS owner_policy (id INTEGER PRIMARY KEY CHECK(id=1), body TEXT NOT NULL)');
  const row = storage.sql.exec<{body:string}>('SELECT body FROM owner_policy WHERE id=1').toArray()[0];
  if (!row) return null;
  const value = JSON.parse(row.body) as Projection;
  if (!value || !Number.isSafeInteger(value.sequence) || !Array.isArray(value.history) || value.history.length>16) throw Error('stored_policy');
  return value;
}
export async function capability(provided: string, configured: string): Promise<void> {
  if (!/^[-_A-Za-z0-9]{43,128}$/u.test(configured) || provided.length>128) throw Error('delivery_capability');
  const actual = await digest(provided), expected = await digest(configured);
  let difference = 0;
  for (let i=0;i<expected.length;i++) difference |= actual.charCodeAt(i) ^ expected.charCodeAt(i);
  if (difference!==0) throw Error('delivery_capability');
}
