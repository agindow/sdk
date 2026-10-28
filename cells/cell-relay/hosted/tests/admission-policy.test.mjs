import {test} from 'node:test';
import assert from 'node:assert/strict';
import {loadRenewal} from './support/load-renewal.mjs';
const now=1800000000;
/** @type {import('../owner/control/admission-policy.js').AdmissionPolicy} */
const p={version:'p1',tenant:'a',environment:'test',origin:'https://guard.invalid',profile:'primary',connection:'c',audience:'https://guard.invalid/v1/token',tools:['op'],until:now+86400,bootstrapGeneration:'generation'};
/** @type {import('../owner/control/protocol.js').Row} */
const row={id:'workload-one',kid:'client',x:'A'.repeat(43),tenant:'a',environment:'test',connection:'c',audience:p.audience,tools:['op']};
/** @param {readonly import('../owner/control/protocol.js').Row[]} rows @returns {import('../owner/control/protocol.js').Command} */
const command=rows=>({version:1,kid:'self',origin:p.origin,profile:p.profile,generation:'generation',previousGeneration:null,expectedRevision:0,id:'one',iat:now,exp:now+120,digest:'digest',action:'admit',reviewUntil:p.until,rows});
/** @returns {import('../owner/control/state.js').ControlState} */
const initial=()=>({generation:'generation',revision:0,floor:now,snapshot:null,pending:[],lastFailure:null,retry:[]});
/** @param {import('../owner/control/protocol.js').Command} c @param {import('../owner/control/protocol.js').Status} status @returns {import('../owner/control/protocol.js').Snapshot} */
const snapshot=(c,status)=>({version:1,status,revision:1,sequence:1,issuedAt:now,notAfter:now+3600,review:{payload:c,signature:'fixture'}});
test('admission reviewer cannot widen tenant, tools or restore a stopped baseline',async()=>{
 const {admissionConstraint}=/** @type {typeof import('../owner/control/admission-policy.js')} */(await loadRenewal('admission-policy'));
 const c=command([row]),state=initial();
 admissionConstraint(p,c,state,now);
 for(const changed of [{tenant:'b'},{tools:['write']},{environment:/** @type {const} */('live')}])assert.throws(()=>admissionConstraint(p,{...c,rows:[{...row,...changed}]},state,now));
 assert.throws(()=>admissionConstraint(p,c,{...state,snapshot:snapshot(c,'stopped')},now));
 assert.throws(()=>admissionConstraint({...p,bootstrapGeneration:'other'},c,state,now));
});
test('one customer change cannot alter another row or extend the prior mandate',async()=>{
 const {admissionConstraint}=/** @type {typeof import('../owner/control/admission-policy.js')} */(await loadRenewal('admission-policy'));
 const old={...row,id:'workload-old'},c=command([old,row]);
 const state={...initial(),snapshot:snapshot(command([old]),'active')};
 admissionConstraint(p,c,state,now);
 assert.throws(()=>admissionConstraint(p,{...c,reviewUntil:p.until+1},state,now));
 assert.throws(()=>admissionConstraint(p,{...c,rows:[{...old,kid:'changed'},row]},state,now));
});
test('public-only intents reject private material before durable storage',async()=>{
 const {intent}=/** @type {typeof import('../owner/control/admission-input.js')} */(await loadRenewal('admission-input'));
 const body={operation:'create',connection:'c',jwk:{kty:'OKP',crv:'Ed25519',kid:'client',x:'A'.repeat(43)}};
 const parsed=intent(body);assert.equal(parsed.operation,'create');if(parsed.operation!=='create')throw Error('intent');assert.equal(parsed.jwk.kid,'client');
 for(const input of [{...body,jwk:{...body.jwk,d:'PRIVATE'}},{...body,privateKey:'PEM'},{...body,jwk:{...body.jwk,x:'PEM'}},{...body,jwk:{...body.jwk,x:'B'.repeat(43)}}])assert.throws(()=>intent(input));
});
