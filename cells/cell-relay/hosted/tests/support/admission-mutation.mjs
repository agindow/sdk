import {readFile} from 'node:fs/promises';
/** @param {string|undefined} selected @returns {import('esbuild').Plugin[]} */
export function admissionMutation(selected){
 if(selected===undefined)return [];
 /** @type {Record<string,readonly [string,string,string]>} */
 const cases={
  flag:['/admission/service.ts',"if(this.env.FEATURE_FLAGS!=='workload-self-serve')throw Error('unavailable');",''],
  orgNamespace:['/admission/service.ts',"if(!this.ctx.id.equals(this.env.ADMISSIONS.idFromName(profile)))throw Error('forbidden');",''],
  orgRows:['/control/host-policy.ts','allowedRows(policy,rows);',''],
  orgHint:['/control/host-policy.ts',"if(rows.some(row=>registrationProfile(row.id)!==profile))throw Error('admission_scope');",''],
  orgNonce:['/control/store.ts','||old.nonce!==nonce',''],
  catalog:['/control/read-catalog.ts',"entry.operation!=='read'",'false'],
  batch:['/control/org-directory.ts','ORDER BY profile LIMIT 16','ORDER BY profile LIMIT 17'],
  rebase:['/admission/service.ts','// Rebase only a proven concurrent revision, revalidating the original row revision below.\n     r.command=await this.#command(r,p,state);this.#store.save(r);','return;'],
  authority:['/control/authority.ts','admissionConstraint(p,c.payload,state,now);',''],
  private:['/control/admission-input.ts',"exact(j,['kty','crv','kid','x']);",''],
  recovery:['/admission/service.ts',"if(baseline.snapshot!==null||baseline.generation!==p.bootstrapGeneration)throw Error('unavailable');",''],
  readiness:['/owner/owner.ts','||now<this.#readyAt', '||false'],
  permission:['/workloads/management.ts',"if(membership.role!=='owner')return refuse();",''],
  ui:['/workloads/model.ts',"w.status==='ready'&&",'true&&'],
  tenant:['/control/admission-store.ts',"if(old&&old.body!==anchor)throw Error('policy_changed');",''],
  immutable:['/control/admission-store.ts',"if(existing&&existing.body!==body)throw Error('policy_changed');",''],
  receipt:['/admission/service.ts','if(ack.generation===c.generation&&ack.revision===c.expectedRevision+1)','if(false)'],
  compact:['/admission/service.ts','signature:text(signed.signature,128)};r.command=null;','signature:text(signed.signature,128)};'],
  intentExpiry:['/admission/service.ts',"if(this.now()>=r.createdAt+300){r.result='rejected';r.command=null;this.#store.save(r);return;}",''],
 };
 const entry=cases[selected];if(!entry)throw Error('unknown admission mutation');
 return [{name:'admission-mutation',setup(plugin){plugin.onLoad({filter:/\.ts$/},async({path})=>{
  if(!path.endsWith(entry[0]))return undefined;
  const source=await readFile(path,'utf8');if(source.split(entry[1]).length!==2)throw Error('admission mutation target changed');
  return {contents:source.replace(entry[1],entry[2]),loader:'ts'};
 });}}];
}
