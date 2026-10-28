import { record } from '../policy.js';
import { exact, integer, text, type Command, type Row } from './protocol.js';
import type { ControlState } from './state.js';
export interface AdmissionPolicy {
 version:string; tenant:string; environment:'test'; origin:string; profile:string;
 connection:string; audience:string; tools:readonly string[]; until:number; bootstrapGeneration:string;
}
export function admissionPolicy(input:unknown):AdmissionPolicy {
 const p=record(input);
 exact(p,['version','tenant','environment','origin','profile','connection','audience','tools','until','bootstrapGeneration']);
 const origin=text(p.origin,2048);
 if(new URL(origin).origin!==origin||!origin.startsWith('https://')||p.environment!=='test'||!Array.isArray(p.tools)||!p.tools.length||p.tools.length>64)throw Error('admission_policy');
 const tools=p.tools.map(t=>text(t));
 if(new Set(tools).size!==tools.length||tools.some(t=>/[/*]/u.test(t)))throw Error('admission_policy');
 const audience=text(p.audience,2048);
 if(audience!==origin+'/v1/token')throw Error('admission_policy');
 return {version:text(p.version),tenant:text(p.tenant),environment:'test',origin,profile:text(p.profile),connection:text(p.connection),audience,tools,until:integer(p.until,1),bootstrapGeneration:text(p.bootstrapGeneration)};
}
export function allowedRows(p:AdmissionPolicy,rows:readonly Row[]):void {
 for(const r of rows)if(!/^workload-[a-zA-Z0-9_-]{1,128}$/u.test(r.id)||r.tenant!==p.tenant||r.environment!==p.environment||r.connection!==p.connection||r.audience!==p.audience||
  r.tools.length!==p.tools.length||r.tools.some(t=>!p.tools.includes(t)))throw Error('admission_scope');
}
/** Independent verifier-side ceiling: customer signer never inherits offline reviewer powers. */
export function admissionConstraint(p:AdmissionPolicy,c:Command,state:ControlState,now:number):void {
 if(p.until<=now||c.generation!==p.bootstrapGeneration||c.reviewUntil>p.until||c.origin!==p.origin||c.profile!==p.profile||c.previousGeneration!==null&&c.previousGeneration!==undefined)throw Error('admission_scope');
 allowedRows(p,c.rows);
 const old=state.snapshot;
 if(!old){
  if(c.action!=='admit'||c.rows.length!==1||c.generation!==p.bootstrapGeneration)throw Error('admission_bootstrap');
  return;
 }
 if(old.status!=='active'||old.notAfter<=now||old.review.payload.reviewUntil<=now||c.reviewUntil>old.review.payload.reviewUntil)throw Error('admission_inactive');
 allowedRows(p,old.review.payload.rows);
 if(c.action==='revoke'){
  if(old.review.payload.rows.length!==1||JSON.stringify(c.rows)!==JSON.stringify(old.review.payload.rows))throw Error('admission_scope');
  return;
 }
 if(c.action!=='admit')throw Error('admission_scope');
 const prior=new Map(old.review.payload.rows.map(r=>[r.id,r]));
 const changed=c.rows.filter(r=>JSON.stringify(r)!==JSON.stringify(prior.get(r.id)));
 const remaining=new Set(c.rows.map(r=>r.id));
 const removed=old.review.payload.rows.filter(r=>!remaining.has(r.id));
 if(changed.length+removed.length!==1)throw Error('admission_scope');
 for(const r of changed){
  const was=prior.get(r.id);
  if(was&&JSON.stringify({...r,kid:was.kid,x:was.x})!==JSON.stringify(was))throw Error('admission_scope');
 }
}
