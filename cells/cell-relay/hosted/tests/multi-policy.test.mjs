import {test} from 'node:test';
import assert from 'node:assert/strict';
import {loadRenewal} from './support/load-renewal.mjs';
const input={format:2,version:'host-v1',reviewerKid:'host-reviewer',origin:'https://guard.invalid',profileKey:'customer',environment:'test',connection:'c',tools:['op'],audience:'https://guard.invalid/v1/token',until:1800086400};
test('host policy has no tenant/generation grant and forbids wildcards and live scopes',async()=>{
 const {hostPolicy}=/** @type {typeof import('../owner/control/host-policy.js')} */(await loadRenewal('host-policy'));
 assert.equal(hostPolicy(input).format,2);
 for(const delta of [{tenant:'a'},{tenant:'*'},{bootstrapGeneration:'g'},{environment:/** @type {const} */('live')},{connection:'*'},{tools:['*']}])assert.throws(()=>hostPolicy({...input,...delta}));
});
test('org profiles and registration hints are deterministic, isolated and verified against rows',async()=>{
 const {hostPolicy,orgProfile,registrationProfile,scopedHostPolicy}=/** @type {typeof import('../owner/control/host-policy.js')} */(await loadRenewal('host-policy'));
 const p=hostPolicy(input),a=await orgProfile(p,'a'),b=await orgProfile(p,'b');assert.notEqual(a,b);assert.equal(a,await orgProfile(p,'a'));
 assert.equal(registrationProfile('workload-'+a+'-id'),a);
 const row={id:'workload-'+a+'-id',kid:'client',x:'A'.repeat(43),audience:p.audience,tenant:'a',environment:/** @type {const} */('test'),connection:'c',tools:['op']};
 const rows=[row];
 for(const change of [{tenant:'b'},{environment:/** @type {const} */('live')},{connection:'other'},{tools:['write']},{audience:'https://other.invalid/v1/token'}])
  await assert.rejects(scopedHostPolicy(p,a,[row,{...row,...change,id:'workload-'+a+'-second'}],'generation'));
 await assert.rejects(scopedHostPolicy(p,a,[{...row,id:'workload-'+b+'-id'}],'generation'));
 assert.equal((await scopedHostPolicy(p,a,rows,'generation')).tenant,'a');
 await assert.rejects(scopedHostPolicy(p,a,[{...row,tenant:'b'}],'generation'));
 await assert.rejects(scopedHostPolicy(p,a,[...rows,{...row,id:'workload-'+b+'-id',tenant:'b'}],'generation'));
});
test('host read catalog materializes tenant-scoped routes and rejects writable or pre-bound entries',async()=>{
 const {readCatalogBindings}=/** @type {typeof import('../owner/control/read-catalog.js')} */(await loadRenewal('read-catalog'));
 const row={id:'workload-org2-'+ 'a'.repeat(64)+'-id',owner:'owner',kid:'k',x:'A'.repeat(43),tenant:'org-a',environment:/** @type {const} */('test'),connection:'c',tools:['op'],audience:input.audience};
 const raw=JSON.stringify({'c/op':{url:'https://vendor.invalid/read',method:'POST',operation:'read'}});
 assert.equal(JSON.parse(readCatalogBindings(raw,row))['c/op'].tenant,'org-a');
 for(const delta of [{operation:'write'},{tenant:'*'},{tenant:'org-b'},{environment:'live'}])assert.throws(()=>readCatalogBindings(JSON.stringify({'c/op':{url:'https://vendor.invalid/read',method:'POST',operation:'read',...delta}}),row));
});
