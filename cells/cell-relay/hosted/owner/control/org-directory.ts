import {DurableObject} from 'cloudflare:workers';
import {hostPolicy,orgProfile} from './host-policy.js';
import type {ControlState} from './state.js';
type Entry={tenant:string;profile:string;nonce:string;generation:string|null};
/** Index contains profile locators/generation pins, never a mixed registration baseline. */
export class OrgProfileDirectory extends DurableObject<KeylessMultiControlBindings>{
 #operations:Promise<unknown>=Promise.resolve();
 constructor(ctx:DurableObjectState,env:KeylessMultiControlBindings){
  super(ctx,env);
  ctx.storage.sql.exec('CREATE TABLE IF NOT EXISTS org_profiles(profile TEXT PRIMARY KEY,tenant TEXT NOT NULL,nonce TEXT NOT NULL,generation TEXT)');
  ctx.storage.sql.exec('CREATE TABLE IF NOT EXISTS org_cycle(id INTEGER PRIMARY KEY CHECK(id=1), cursor TEXT, floor INTEGER NOT NULL)');
  ctx.storage.sql.exec('INSERT OR IGNORE INTO org_cycle VALUES(1,NULL,0)');
 }
 protected now():number{return Math.floor(Date.now()/1000);}
 #serial<T>(work:()=>Promise<T>):Promise<T>{const p=this.#operations.then(work);this.#operations=p.catch(()=>undefined);return p;}
 #enabled(){
  if(this.env.KEYLESS_CONTROL_ENABLED!=='true'||!this.ctx.id.equals(this.env.ORG_DIRECTORY.idFromName('host-directory')))throw Error('directory_disabled');
  const p=hostPolicy(JSON.parse(this.env.KEYLESS_HOST_POLICY));if(p.until<=this.now()||p.origin!==this.env.EXECUTION_ORIGIN)throw Error('host_policy');
  const floor=this.ctx.storage.sql.exec<{floor:number}>('SELECT floor FROM org_cycle WHERE id=1').one().floor;
  if(this.now()<floor)throw Error('clock');this.ctx.storage.sql.exec('UPDATE org_cycle SET floor=? WHERE id=1',this.now());return p;
 }
 async inspect(tenant:string,provision:boolean):Promise<string>{return this.#serial(async()=>{
  const p=this.#enabled(),profile=await orgProfile(p,tenant);
  let entry=this.ctx.storage.sql.exec<Entry>('SELECT * FROM org_profiles WHERE profile=?',profile).toArray()[0];
  if(!entry){
   if(!provision)return 'null';
   if(this.ctx.storage.sql.exec<{n:number}>('SELECT count(*) AS n FROM org_profiles').one().n>=1024)throw Error('capacity');
   entry={tenant,profile,nonce:crypto.randomUUID(),generation:null};
   // Persist a nonce reservation before RPC; restart can finish it, directory loss cannot adopt it.
   this.ctx.storage.sql.exec('INSERT INTO org_profiles VALUES(?,?,?,NULL)',profile,tenant,entry.nonce);
  }
  if(entry.tenant!==tenant)throw Error('org_binding');
  const opened=JSON.parse(await this.env.POLICIES.getByName(profile).openOrg(tenant,entry.nonce)) as {state:ControlState};
  if(entry.generation!==null&&entry.generation!==opened.state.generation)throw Error('org_generation');
  if(entry.generation===null)this.ctx.storage.sql.exec('UPDATE org_profiles SET generation=? WHERE profile=?',opened.state.generation,profile);
  return JSON.stringify(opened.state);
 });}
 async submit(tenant:string,command:string):Promise<string>{
  const state:unknown=JSON.parse(await this.inspect(tenant,false));if(state===null)throw Error('org_unenrolled');
  const p=this.#enabled();return this.env.POLICIES.getByName(await orgProfile(p,tenant)).submit(command);
 }
 async recover(tenant:string,command:string):Promise<string>{
  const state:unknown=JSON.parse(await this.inspect(tenant,false));if(state===null)throw Error('org_unenrolled');
  const p=this.#enabled();return this.env.POLICIES.getByName(await orgProfile(p,tenant)).recover(command);
 }
 async renewCurrent():Promise<string>{return this.#serial(async()=>{
  this.#enabled();
  const cycle=this.ctx.storage.sql.exec<{cursor:string|null}>('SELECT cursor FROM org_cycle WHERE id=1').one();
  if(cycle.cursor===null)this.ctx.storage.sql.exec("UPDATE org_cycle SET cursor='' WHERE id=1");
  return this.#batch();
 });}
 async #batch():Promise<string>{
  this.#enabled();const cycle=this.ctx.storage.sql.exec<{cursor:string|null}>('SELECT cursor FROM org_cycle WHERE id=1').one();
  if(cycle.cursor===null)return JSON.stringify({processed:0,pending:false});
  const entries=this.ctx.storage.sql.exec<Entry>('SELECT * FROM org_profiles WHERE generation IS NOT NULL AND profile>? ORDER BY profile LIMIT 16',cycle.cursor).toArray();
  for(const entry of entries){
   try{await this.env.POLICIES.getByName(entry.profile).renewCurrent();}catch{/* Next scheduled cycle retries; no failed profile can starve another. */}
  }
  const last=entries.at(-1)?.profile??cycle.cursor;
  const more=this.ctx.storage.sql.exec<{n:number}>('SELECT count(*) AS n FROM org_profiles WHERE generation IS NOT NULL AND profile>?',last).one().n>0;
  this.ctx.storage.sql.exec('UPDATE org_cycle SET cursor=? WHERE id=1',more?last:null);
  if(more)await this.ctx.storage.setAlarm((this.now()+1)*1000);
  return JSON.stringify({processed:entries.length,pending:more});
 }
 override async alarm():Promise<void>{await this.#serial(async()=>{try{await this.#batch();}catch{await this.ctx.storage.setAlarm((this.now()+60)*1000);}});}
}
