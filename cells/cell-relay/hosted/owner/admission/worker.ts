import { hostPolicy, orgProfile } from '../control/host-policy.js';
import { identity, intent } from '../control/admission-input.js';
import { WorkerEntrypoint } from 'cloudflare:workers';
export { WorkloadAdmission } from './service.js';
/** Bind only the session-verifying identity Worker to this named private entrypoint. */
export class CustomerWorkloads extends WorkerEntrypoint<WorkloadAdmissionBindings|MultiWorkloadAdmissionBindings> {
 async #profile(raw:string,recent:boolean):Promise<string>{
  if(!('KEYLESS_MULTI_ORG' in this.env))return this.env.KEYLESS_PROFILE;
  if(this.env.FEATURE_FLAGS!=='workload-self-serve'||this.env.KEYLESS_MULTI_ORG!=='true'||raw.length>4096)throw Error('unavailable');
  const a=identity(JSON.parse(raw),Math.floor(Date.now()/1000),recent);return orgProfile(hostPolicy(JSON.parse(this.env.ADMISSION_POLICY)),a.tenant);
 }
 async list(raw:string):Promise<string>{return this.env.ADMISSIONS.getByName(await this.#profile(raw,false)).list(raw);}
 async mutate(raw:string,id:string,input:string):Promise<string>{
  if(new TextEncoder().encode(input).byteLength>4096)throw Error('invalid_intent');intent(JSON.parse(input));
  return this.env.ADMISSIONS.getByName(await this.#profile(raw,true)).mutate(raw,id,input);
 }
}
export default {fetch():Response{return new Response(null,{status:404});}};
