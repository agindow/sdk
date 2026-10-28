import {digest,record} from '../policy.js';
import {exact,integer,text,type Row} from './protocol.js';
import {allowedRows,type AdmissionPolicy} from './admission-policy.js';
export interface HostPolicy {
 format:2;version:string;reviewerKid:string;origin:string;profileKey:string;environment:'test';
 connection:string;tools:readonly string[];audience:string;until:number;
}
export interface ResolvedPolicy extends AdmissionPolicy {host?:HostPolicy}
export function isHostPolicy(input:unknown):boolean{return typeof input==='object'&&input!==null&&'format' in input&&input.format===2;}
export function hostPolicy(input:unknown):HostPolicy {
 const p=record(input);exact(p,['format','version','reviewerKid','origin','profileKey','environment','connection','tools','audience','until']);
 const origin=text(p.origin,2048),profileKey=text(p.profileKey,64),connection=text(p.connection);
 if(p.format!==2||p.environment!=='test'||new URL(origin).origin!==origin||!origin.startsWith('https://')||!/^[-_a-zA-Z0-9]+$/u.test(profileKey)||/[/*]/u.test(connection))throw Error('host_policy');
 if(!Array.isArray(p.tools)||!p.tools.length||p.tools.length>64)throw Error('host_policy');
 const tools=p.tools.map(t=>text(t));
 if(new Set(tools).size!==tools.length||tools.some(t=>/[/*]/u.test(t)))throw Error('host_policy');
 const audience=text(p.audience,2048);if(audience!==origin+'/v1/token')throw Error('host_policy');
 return {format:2,version:text(p.version),reviewerKid:text(p.reviewerKid),origin,profileKey,environment:'test',connection,tools,audience,until:integer(p.until,1)};
}
export async function orgProfile(p:HostPolicy,tenant:string):Promise<string>{
 text(tenant,128);if(!/^[-_a-zA-Z0-9]+$/u.test(tenant))throw Error('tenant');
 return 'org2-'+await digest(JSON.stringify(['agindow-org-profile-v2',p.origin,p.profileKey,tenant,'test']));
}
export function registrationProfile(id:string):string|undefined{return /^workload-(org2-[a-f0-9]{64})-[-_a-zA-Z0-9]{1,128}$/u.exec(id)?.[1];}
export function resolvedPolicy(p:HostPolicy,tenant:string,profile:string,generation:string):ResolvedPolicy {
 return {version:p.version,tenant,environment:'test',origin:p.origin,profile,connection:p.connection,tools:p.tools,audience:p.audience,until:p.until,bootstrapGeneration:generation,host:p};
}
export async function scopedHostPolicy(p:HostPolicy,profile:string,rows:readonly Row[],generation:string):Promise<ResolvedPolicy>{
 const first=rows[0];if(!first||await orgProfile(p,first.tenant)!==profile)throw Error('admission_scope');
 const policy=resolvedPolicy(p,first.tenant,profile,generation);allowedRows(policy,rows);
 if(rows.some(row=>registrationProfile(row.id)!==profile))throw Error('admission_scope');return policy;
}
