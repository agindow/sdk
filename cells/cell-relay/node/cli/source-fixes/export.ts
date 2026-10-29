import { copyBounded, freezeValue, parseSourceRepairEvidence } from 'contracts';
import { record } from '../binding/input.js';
import { generateSourcePatch } from '../migration/index.js';
import { refused } from '../migration/types.js';
export interface LocalEvidenceSource {read(reference:string):Promise<unknown>}
/** Local maintainer assertions only. No hosted authority, publisher or automatic test runner. */
export async function exportSourceFix(value:unknown,now:number,source:LocalEvidenceSource) {
 let timer:ReturnType<typeof setTimeout>|undefined;
 try{
  const input=freezeValue(copyBounded(value,{bytes:65536,nodes:4096,depth:12}));
  if(!record(input)||typeof input['evidenceReference']!=='string'||!input['evidenceReference']||input['evidenceReference'].length>1024
   ||Object.keys(input).some(key=>!['checkout','binding','grant','evidenceReference'].includes(key)))return refused('invalid_selection');
  const raw=await Promise.race([source.read(input['evidenceReference']),new Promise<never>((_,reject)=>{timer=setTimeout(()=>reject(Error('Evidence unavailable')),5000);})]);
  const evidence=freezeValue(copyBounded(raw,{bytes:32768,nodes:4096,depth:12}));
  const result=await generateSourcePatch({checkout:input['checkout'],binding:input['binding'],grant:input['grant'],evidence},now);
  if(result.status==='refused')return result;
  const admitted=parseSourceRepairEvidence(evidence);
  return {...result,citations:admitted.facts.map(fact=>({factId:fact.id,from:fact.from,to:fact.to,...fact.provenance})),
   tests:{state:'unvalidated' as const,reason:'No independent validation host configured.',receipt:null},
   publicationAuthorized:false as const,providerCI:'unverified' as const,vendorAcceptance:'unverified' as const,businessWrite:'unverified' as const};
 }catch{return refused('invalid_selection');}
 finally{if(timer!==undefined)clearTimeout(timer);}
}
