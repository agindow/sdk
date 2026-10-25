import { jsonText } from './json-text.js';
import { parseAuditArtifact } from './validate.js';
import type { AuditArtifact } from './generated/audit_artifact.js';
export const AUDIT_INSPECTION_BYTES=1048576;
export interface AuditInspection {readonly text:string;readonly artifact:AuditArtifact;readonly display:string;readonly recurrence:string|null}
export function inspectAuditText(text:string):AuditInspection {
 if(new TextEncoder().encode(text).length>AUDIT_INSPECTION_BYTES)throw Error('Audit artifact exceeds inspection bound');
 const value:unknown=JSON.parse(text);
 const pending:{value:unknown;depth:number}[]=[{value,depth:0}];let nodes=0;
 while(pending.length){const item=pending.pop();if(!item)break;if(++nodes>20000||item.depth>20)throw Error('Audit structure exceeds bound');
  if(typeof item.value==='object'&&item.value!==null)for(const child of Object.values(item.value) as unknown[])pending.push({value:child,depth:item.depth+1});
 }
 const artifact=parseAuditArtifact(value),patch=artifact.patch;
 if(patch&&(patch.applied===patch.byte_identical||patch.applied&&!patch.fields_changed?.length))throw Error('Inconsistent patch evidence');
 if(artifact.decision==='repaired'&&!patch?.applied)throw Error('Missing applied patch evidence');
 if((artifact.drift?.first_seen===undefined)!==(artifact.drift?.occurrences===undefined))throw Error('Incomplete recurrence evidence');
 const tree=jsonText(text);const recurrence=tree.members.get('drift')?.members.get('occurrences')?.raw??null;
 return {text,artifact,display:text,recurrence};
}
export function patchSides(artifact:AuditArtifact):{request:string[];response:string[];unattributed:string[]} {
 const result:{request:string[];response:string[];unattributed:string[]}={request:[],response:[],unattributed:[]};
 if(!artifact.patch?.applied)return result;
 for(const field of artifact.patch.fields_changed??[]){
  if(field.startsWith('request:'))result.request.push(field.slice(8));else if(field.startsWith('response:'))result.response.push(field.slice(9));else result.unattributed.push(field);
 }return result;
}
