import type { AdmissionPolicy } from './admission-policy.js';
import type { DurableObjectStorage } from '@cloudflare/workers-types';
import type { Intent, IdentityIntent } from './admission-input.js';
import type { Row } from './protocol.js';
export interface Receipt {
 id:string;digest:string;identity:IdentityIntent;intent:Intent;registration:string;policy:string;
 createdAt:number;command:string|null;provenance:{encoded:string;signature:string}|null;revision:number|null;result:'submitted'|'admitted'|'rejected';
}
export interface Workload {id:string;revision:number;row:Row;revoked:boolean;receipt:string}
/** Public intents and signed public commands only. SQL capacity never evicts live idempotency fences. */
export class AdmissionStore {
 constructor(readonly storage:DurableObjectStorage){
  storage.sql.exec('CREATE TABLE IF NOT EXISTS workload_tenant(id INTEGER PRIMARY KEY CHECK(id=1), tenant TEXT NOT NULL)');
  storage.sql.exec('CREATE TABLE IF NOT EXISTS workload_intents(id TEXT PRIMARY KEY, body TEXT NOT NULL)');
  storage.sql.exec('CREATE TABLE IF NOT EXISTS workloads(id TEXT PRIMARY KEY, body TEXT NOT NULL)');
  storage.sql.exec('CREATE TABLE IF NOT EXISTS workload_policy_anchor(id INTEGER PRIMARY KEY CHECK(id=1), body TEXT NOT NULL)');
  storage.sql.exec('CREATE TABLE IF NOT EXISTS workload_policy_versions(version TEXT PRIMARY KEY, body TEXT NOT NULL)');
  storage.sql.exec('CREATE TABLE IF NOT EXISTS workload_clock(id INTEGER PRIMARY KEY CHECK(id=1), floor INTEGER NOT NULL)');
 }
 tenant():string|undefined{return this.storage.sql.exec<{tenant:string}>('SELECT tenant FROM workload_tenant WHERE id=1').toArray()[0]?.tenant;}
 bindTenant(tenant:string):void{const old=this.tenant();if(old!==undefined&&old!==tenant)throw Error('org_binding');this.storage.sql.exec('INSERT OR IGNORE INTO workload_tenant VALUES(1,?)',tenant);}
 bindPolicy(p:AdmissionPolicy):void {
  this.storage.transactionSync(()=>{
   const anchor=JSON.stringify([p.tenant,p.environment,p.origin,p.profile,p.bootstrapGeneration]);
   const old=this.storage.sql.exec<{body:string}>('SELECT body FROM workload_policy_anchor WHERE id=1').toArray()[0];
   if(old&&old.body!==anchor)throw Error('policy_changed');
   const versions=this.storage.sql.exec<{version:string;body:string}>('SELECT version,body FROM workload_policy_versions').toArray();
   const existing=versions.find(v=>v.version===p.version),body=JSON.stringify(p);
   if(existing&&existing.body!==body)throw Error('policy_changed');
   if(!existing&&versions.length>=32)throw Error('capacity');
   this.storage.sql.exec('INSERT OR IGNORE INTO workload_policy_anchor VALUES(1,?)',anchor);
   this.storage.sql.exec('INSERT OR IGNORE INTO workload_policy_versions VALUES(?,?)',p.version,body);
  });
 }
 floor(now:number):void {
  this.storage.transactionSync(()=>{
   const old=this.storage.sql.exec<{floor:number}>('SELECT floor FROM workload_clock WHERE id=1').toArray()[0];
   if(old&&now<old.floor)throw Error('clock');
   this.storage.sql.exec('INSERT INTO workload_clock VALUES(1,?) ON CONFLICT(id) DO UPDATE SET floor=excluded.floor',now);
  });
 }
 receipts():Receipt[]{return this.storage.sql.exec<{body:string}>('SELECT body FROM workload_intents ORDER BY id').toArray().map(r=>JSON.parse(r.body) as Receipt);}
 workloads():Workload[]{return this.storage.sql.exec<{body:string}>('SELECT body FROM workloads ORDER BY id').toArray().map(r=>JSON.parse(r.body) as Workload);}
 save(r:Receipt):void {this.storage.sql.exec('INSERT INTO workload_intents VALUES(?,?) ON CONFLICT(id) DO UPDATE SET body=excluded.body',r.id,JSON.stringify(r));}
 saveWorkload(w:Workload):void {this.storage.sql.exec('INSERT INTO workloads VALUES(?,?) ON CONFLICT(id) DO UPDATE SET body=excluded.body',w.id,JSON.stringify(w));}
}
