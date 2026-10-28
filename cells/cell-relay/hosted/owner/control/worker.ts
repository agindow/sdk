import { WorkerEntrypoint } from 'cloudflare:workers';
export { KeylessPolicyAuthority } from './authority.js';
/** Scheduler capability: no review mutation, command transport or execution-owner binding. */
export class Renewal extends WorkerEntrypoint<KeylessControlBindings> {
  renewCurrent(): Promise<string> { return this.env.POLICIES.getByName(this.env.KEYLESS_PROFILE).renewCurrent(); }
  health(): Promise<string> { return this.env.POLICIES.getByName(this.env.KEYLESS_PROFILE).status(); }
}
/** Deliberately distinct private entrypoint; commands still require a reviewer signature. */
export class Administration extends WorkerEntrypoint<KeylessControlBindings> {
  inspect(): Promise<string> { return this.env.POLICIES.getByName(this.env.KEYLESS_PROFILE).inspect(); }
  recover(command:string):Promise<string> {return this.env.POLICIES.getByName(this.env.KEYLESS_PROFILE).recover(command);}
  submit(command: string): Promise<string> { return this.env.POLICIES.getByName(this.env.KEYLESS_PROFILE).submit(command); }
}
export default { fetch(): Response { return new Response(null,{status:404}); } };
export {OrgProfileDirectory} from './org-directory.js';
/** Host admission capability. No public HTTP route and no signing key on this Worker. */
export class MultiAdministration extends WorkerEntrypoint<KeylessMultiControlBindings>{
 inspect(tenant:string,provision=false):Promise<string>{return this.env.ORG_DIRECTORY.getByName('host-directory').inspect(tenant,provision);}
 submit(tenant:string,command:string):Promise<string>{return this.env.ORG_DIRECTORY.getByName('host-directory').submit(tenant,command);}
 recover(tenant:string,command:string):Promise<string>{return this.env.ORG_DIRECTORY.getByName('host-directory').recover(tenant,command);}
}
/** Renewal can visit the existing index only; it accepts neither tenants nor rows. */
export class MultiRenewal extends WorkerEntrypoint<KeylessMultiControlBindings>{
 renewCurrent():Promise<string>{return this.env.ORG_DIRECTORY.getByName('host-directory').renewCurrent();}
 health():Promise<string>{return Promise.resolve(JSON.stringify({mode:'enrolled-profiles-only'}));}
}
