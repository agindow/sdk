import {readCatalogBindings} from '../control/read-catalog.js';
import { hostPolicy, orgProfile, resolvedPolicy, type ResolvedPolicy } from '../control/host-policy.js';
import { DurableObject } from 'cloudflare:workers';
import { digest, record } from '../policy.js';
import { admissionConstraint, admissionPolicy, type AdmissionPolicy } from '../control/admission-policy.js';
import { identity, intent, publicJwk } from '../control/admission-input.js';
import { AdmissionStore, type Receipt } from '../control/admission-store.js';
import { encoded, reviewed, text, type Command, type Row } from '../control/protocol.js';
import type { ControlState } from '../control/state.js';
export class WorkloadAdmission extends DurableObject<WorkloadAdmissionBindings|MultiWorkloadAdmissionBindings> {
 readonly #store:AdmissionStore;
 #operations:Promise<unknown>=Promise.resolve();
 constructor(ctx:DurableObjectState,env:WorkloadAdmissionBindings|MultiWorkloadAdmissionBindings){super(ctx,env);this.#store=new AdmissionStore(ctx.storage);}
 protected now():number{return Math.floor(Date.now()/1000);}
 #serial<T>(work:()=>Promise<T>):Promise<T>{const p=this.#operations.then(work);this.#operations=p.catch(()=>undefined);return p;}
 async #initialize(raw:string,recent:boolean):Promise<void>{
  if(!('KEYLESS_MULTI_ORG' in this.env))return;
  if(this.env.FEATURE_FLAGS!=='workload-self-serve'||this.env.KEYLESS_MULTI_ORG!=='true')throw Error('unavailable');
  if(raw.length>4096)throw Error('forbidden');const a=identity(JSON.parse(raw),this.now(),recent),p=hostPolicy(JSON.parse(this.env.ADMISSION_POLICY));
  const profile=await orgProfile(p,a.tenant);
  if(!this.ctx.id.equals(this.env.ADMISSIONS.idFromName(profile)))throw Error('forbidden');
  this.#store.bindTenant(a.tenant);
 }
 async #inspect(provision=false):Promise<string>{
  if('KEYLESS_MULTI_ORG' in this.env){const tenant=this.#store.tenant();if(this.env.KEYLESS_MULTI_ORG!=='true'||!tenant)throw Error('unavailable');return this.env.ADMINISTRATION.inspect(tenant,provision);}
  return this.env.ADMINISTRATION.inspect();
 }
 async #submit(raw:string):Promise<string>{
  if('KEYLESS_MULTI_ORG' in this.env){const tenant=this.#store.tenant();if(!tenant)throw Error('unavailable');return this.env.ADMINISTRATION.submit(tenant,raw);}
  return this.env.ADMINISTRATION.submit(raw);
 }
 async #recover(raw:string):Promise<string>{
  if('KEYLESS_MULTI_ORG' in this.env){const tenant=this.#store.tenant();if(!tenant)throw Error('unavailable');return this.env.ADMINISTRATION.recover(tenant,raw);}
  return this.env.ADMINISTRATION.recover(raw);
 }
 async #policy(provision=false):Promise<ResolvedPolicy> {
  if(this.env.FEATURE_FLAGS!=='workload-self-serve')throw Error('unavailable');
  let p:ResolvedPolicy;
  if('KEYLESS_MULTI_ORG' in this.env){
   const host=hostPolicy(JSON.parse(this.env.ADMISSION_POLICY)),tenant=this.#store.tenant();
   if(!tenant||host.reviewerKid!==this.env.ADMISSION_REVIEW_KID||host.until<=this.now())throw Error('unavailable');
   readCatalogBindings(this.env.KEYLESS_READ_CATALOG,{id:'catalog-check',kid:'catalog-check',x:'',owner:'',tenant,environment:'test',connection:host.connection,tools:host.tools,audience:host.audience});
   const profile=await orgProfile(host,tenant),state=JSON.parse(await this.#inspect(provision)) as ControlState|null;
   p=resolvedPolicy(host,tenant,profile,state?.generation??'unprovisioned');
   if(state===null){this.#store.floor(this.now());return p;}
  }else{
   if(!this.ctx.id.equals(this.env.ADMISSIONS.idFromName(this.env.KEYLESS_PROFILE)))throw Error('unavailable');
   p=admissionPolicy(JSON.parse(this.env.ADMISSION_POLICY));if(p.profile!==this.env.KEYLESS_PROFILE)throw Error('unavailable');
  }
  if(p.until<=this.now())throw Error('unavailable');
  this.#store.bindPolicy(p);this.#store.floor(this.now());return p;
 }
 #identity(raw:string,p:AdmissionPolicy,recent=true){
  if(raw.length>4096)throw Error('forbidden');
  const a=identity(JSON.parse(raw),this.now(),recent);if(a.tenant!==p.tenant)throw Error('forbidden');return a;
 }
 async mutate(rawIdentity:string,id:string,raw:string):Promise<string>{return this.#serial(async()=>{
  if(new TextEncoder().encode(raw).byteLength>4096)throw Error('invalid_intent');
  const input=intent(JSON.parse(raw));text(id,128);await this.#initialize(rawIdentity,true);
  let p=await this.#policy();
  if(input.operation==='create'&&input.connection!==p.connection)throw Error('forbidden');
  if(input.operation==='create')p=await this.#policy(true);
  if(p.bootstrapGeneration==='unprovisioned')throw Error('conflict');
  const a=this.#identity(rawIdentity,p),now=this.now();
  const requestDigest=await digest(JSON.stringify([a.actor,a.tenant,a.environment,input]));
  const receipts=this.#store.receipts(),prior=receipts.find(r=>r.id===id);
  if(prior){if(prior.digest!==requestDigest)throw Error('conflict');await this.#reconcile(prior,p);return this.#view(p);}
  // An uncertain signed commit blocks later mutations until its receipt is reconciled.
  for(const pending of receipts.filter(r=>r.result==='submitted')){await this.#reconcile(pending,p);if(pending.result==='submitted')throw Error('unavailable');}
  if(receipts.length===0){
   const baseline=JSON.parse(await this.#inspect()) as ControlState;
   if(baseline.snapshot!==null||baseline.generation!==p.bootstrapGeneration)throw Error('unavailable');
  }
  if(receipts.length>=1024)throw Error('capacity');
  const hour=receipts.filter(r=>r.createdAt>now-3600);
  if(hour.length>=50||hour.filter(r=>r.identity.actor===a.actor).length>=10)throw Error('rate_limited');
  const workloads=this.#store.workloads();
  const existing=input.operation==='create'?undefined:workloads.find(w=>w.id===input.id);
  if(input.operation!=='create'&&(!existing||existing.revoked||existing.revision!==input.revision))throw Error('conflict');
  if(input.operation==='create'&&input.connection!==p.connection)throw Error('forbidden');
  const r:Receipt={id,digest:requestDigest,identity:a,intent:input,registration:existing?.id??'workload-'+(p.host?p.profile+'-':'')+crypto.randomUUID(),policy:p.version,createdAt:now,command:null,provenance:null,revision:null,result:'submitted'};
  // Arm before persisting: a crash cannot strand a durable unsigned intent without a wakeup.
  await this.ctx.storage.setAlarm((now+1)*1000);
  this.#store.save(r);
  await this.#reconcile(r,p);return this.#view(p);
 });}
 async #reconcile(r:Receipt,p:AdmissionPolicy):Promise<void>{
  if(r.result!=='submitted')return;
  try{
   let state=JSON.parse(await this.#inspect()) as ControlState;
   if(r.command){
    const c=record(JSON.parse(r.command)).payload as Command;
    if(state.snapshot?.review.payload.id===c.id){this.#finish(r,c);return;}
    const conflict=state.revision!==c.expectedRevision||state.generation!==c.generation;
    if(conflict&&await this.#receipt(r,c)!=='absent')return;
    if(c.exp<=this.now()||this.now()>=r.createdAt+300){r.result='rejected';r.command=null;this.#store.save(r);return;}
    if(conflict){r.command=await this.#command(r,p,state);this.#store.save(r);}
   }else{
    if(this.now()>=r.createdAt+300){r.result='rejected';r.command=null;this.#store.save(r);return;}
    const command=await this.#command(r,p,state);r.command=command;this.#store.save(r);
   }
   for(let attempt=0;attempt<3;attempt++){
    try{
     if(!r.command)throw Error('unavailable');
     const result=record(JSON.parse(await this.#submit(r.command)));
     const c=record(JSON.parse(r.command)).payload as Command;
     if(result.generation!==c.generation||result.revision!==c.expectedRevision+1)throw Error('unavailable');
     this.#finish(r,c);return;
    }catch{
     state=JSON.parse(await this.#inspect()) as ControlState;
     const c=record(JSON.parse(r.command??'{}')).payload as Command;
     if(state.snapshot?.review.payload.id===c.id){this.#finish(r,c);return;}
     if(await this.#receipt(r,c)!=='absent')return;
     if(c.exp<=this.now()||this.now()>=r.createdAt+300){r.result='rejected';r.command=null;this.#store.save(r);return;}
     if(state.revision===c.expectedRevision)return;
     // Rebase only a proven concurrent revision, revalidating the original row revision below.
     r.command=await this.#command(r,p,state);this.#store.save(r);
    }
   }
  }catch(error){
   const reason=error instanceof Error?error.message:'';
   if(['admission_scope','admission_bootstrap','admission_inactive','conflict','policy_changed'].includes(reason)){r.result='rejected';r.command=null;this.#store.save(r);}
   // Fixed statuses only; raw requests and signed bodies never enter diagnostics.
  }
 }
 async #receipt(r:Receipt,c:Command):Promise<'committed'|'absent'|'unknown'>{
  if(!r.command)return 'unknown';
  try{
   const value:unknown=JSON.parse(await this.#recover(r.command));
   if(value===null)return 'absent';
   const ack=record(value);
   if(ack.generation===c.generation&&ack.revision===c.expectedRevision+1){this.#finish(r,c);return 'committed';}
  }catch{/* Unprovable outcomes remain pending; absence after retention is not proof of rejection. */}
  return 'unknown';
 }
 async #command(r:Receipt,p:AdmissionPolicy,state:ControlState):Promise<string>{
  if(r.policy!==p.version)throw Error('policy_changed');
  const now=this.now(),input=r.intent,old=state.snapshot?.review.payload;
  const rows:Row[]=[...(old?.rows??[])];
  if(input.operation==='create')rows.push({id:r.registration,kid:input.jwk.kid,x:input.jwk.x,audience:p.audience,tenant:p.tenant,environment:'test',connection:p.connection,tools:p.tools});
  else{
   const current=this.#store.workloads().find(w=>w.id===r.registration);
   if(!current||current.revoked||current.revision!==input.revision)throw Error('conflict');
   const index=rows.findIndex(row=>row.id===r.registration);
   if(index<0||JSON.stringify(rows[index])!==JSON.stringify(current.row))throw Error('conflict');
   if(input.operation==='rotate')rows[index]={...current.row,kid:input.jwk.kid,x:input.jwk.x};
   else if(rows.length>1)rows.splice(index,1);
  }
  const content=await reviewed(p.origin,p.profile,rows);
  const c:Command={version:1,origin:p.origin,profile:p.profile,generation:state.generation,previousGeneration:null,expectedRevision:state.revision,id:crypto.randomUUID(),kid:this.env.ADMISSION_REVIEW_KID,
   iat:now,exp:Math.min(now+120,r.createdAt+300),action:input.operation==='revoke'&&rows.length===1&&rows[0]?.id===r.registration?'revoke':'admit',reviewUntil:Math.min(p.until,old?.reviewUntil??now+2592000),...content};
  admissionConstraint(p,c,state,now);
  const key:unknown=JSON.parse(this.env.ADMISSION_SIGNING_JWK);
  const privateKey=record(key),pub=publicJwk({kty:privateKey.kty,crv:privateKey.crv,kid:this.env.ADMISSION_REVIEW_KID,x:privateKey.x});
  const signing=await crypto.subtle.importKey('jwk',{kty:pub.kty,crv:pub.crv,x:pub.x,d:text(privateKey.d)},{name:'Ed25519'},false,['sign']);
  const signature=new Uint8Array(await crypto.subtle.sign('Ed25519',signing,new TextEncoder().encode(encoded(c))));
  return JSON.stringify({payload:c,signature:btoa(String.fromCharCode(...signature)).replace(/=/gu,'').replace(/\+/gu,'-').replace(/\//gu,'_')});
 }
 #finish(r:Receipt,c:Command):void{
  const previous=this.#store.workloads().find(w=>w.id===r.registration),row=c.rows.find(row=>row.id===r.registration)??previous?.row;
  if(!row)throw Error('unavailable');
  const signed=record(JSON.parse(r.command??'{}'));
  r.provenance={encoded:encoded(c),signature:text(signed.signature,128)};r.command=null;
  r.revision=c.expectedRevision+1;r.result='admitted';
  this.ctx.storage.transactionSync(()=>{
   this.#store.save(r);this.#store.saveWorkload({id:r.registration,revision:r.revision??0,row,revoked:r.intent.operation==='revoke',receipt:r.id});
  });
 }
 async list(rawIdentity:string):Promise<string>{return this.#serial(async()=>{
  await this.#initialize(rawIdentity,false);const p=await this.#policy();this.#identity(rawIdentity,p,false);return this.#view(p);
 });}
 async #view(p:AdmissionPolicy):Promise<string>{
  if(p.bootstrapGeneration==='unprovisioned')return JSON.stringify({enabled:true,connection:p.connection,tools:p.tools,workloads:[],requests:[]});
  let state:ControlState|undefined;
  try{state=JSON.parse(await this.#inspect()) as ControlState;}catch{/* No optimistic readiness. */}
  const receipts=this.#store.receipts(),policies=new Map(receipts.map(r=>[r.id,r.policy]));
  const workloads=[];
  for(const w of this.#store.workloads()){
   let status=w.revoked?'revoked':'unavailable',checkedAt=0,expiresAt=0;
   const s=state?.snapshot;
   const policy=policies.get(w.receipt)??'unknown';
   if(!w.revoked&&s&&policy===p.version){
    const row=s.review.payload.rows.find(r=>r.id===w.id);
    if(s.status==='revoked')status='revoked';
    else if(s.status!=='active'||s.notAfter<=this.now())status='expired';
    else if(row&&JSON.stringify(row)===JSON.stringify(w.row)){
     status=state?.pending.includes(w.id)?'admitted':'delivered';
     if(status==='delivered')try{
      const ready=record(JSON.parse(await this.env.EXECUTORS.getByName(await digest('agindow-owner-v1:'+w.id)).exchangeReadiness(this.env.ADMISSION_READINESS_TOKEN,w.id)));
      if(ready.ready===true&&ready.generation===state?.generation&&ready.revision===s.revision&&ready.sequence===s.sequence&&typeof ready.checkedAt==='number'&&ready.checkedAt<=this.now()&&this.now()-ready.checkedAt<=30&&typeof ready.expiresAt==='number'&&ready.expiresAt>this.now()){
       status='ready';checkedAt=ready.checkedAt;expiresAt=Math.min(ready.expiresAt,checkedAt+30);
      }
     }catch{/* Delivered alone is insufficient. */}
    }
   }
   workloads.push({id:w.id,revision:w.revision,kid:w.row.kid,status,checkedAt,expiresAt,receipt:w.receipt,policy,
    exchangeUrl:p.origin+'/v1/token',guardUrl:p.origin+'/v1/guard',clientId:w.id,audience:w.row.audience,connection:w.row.connection,tools:w.row.tools});
  }
  return JSON.stringify({enabled:true,connection:p.connection,tools:p.tools,workloads,requests:receipts.filter(r=>r.result!=='admitted').map(r=>({id:r.id,registration:r.registration,status:r.result==='submitted'&&this.now()>=r.createdAt+300?'unavailable':r.result}))});
 }
 override async alarm():Promise<void>{
  await this.#serial(async()=>{
   try{const p=await this.#policy();for(const r of this.#store.receipts().filter(r=>r.result==='submitted').slice(0,16))await this.#reconcile(r,p);}
   finally{if(this.env.FEATURE_FLAGS==='workload-self-serve'&&this.#store.receipts().some(r=>r.result==='submitted'&&this.now()<r.createdAt+300))await this.ctx.storage.setAlarm((this.now()+60)*1000);}
  });
 }
}
