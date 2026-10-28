import {test} from 'node:test';
import assert from 'node:assert/strict';
import {generateKeyPairSync} from 'node:crypto';
import {ownerHarness,credential} from './support/owner-harness.mjs';
/** @param {number} now @param {string} tenant */
const actor=(now,tenant)=>JSON.stringify({actor:'owner-'+tenant,tenant,environment:'test',role:'owner',authenticatedAt:now,authorizedAt:now});
/** @param {import('node:crypto').KeyObject} key */
const create=key=>JSON.stringify({operation:'create',connection:'c',jwk:{kty:'OKP',crv:'Ed25519',kid:'client-key',x:key.export({format:'jwk'}).x}});
test('one tenant-free host policy admits two isolated organizations and executes real reads',async()=>{
 const f=await ownerHarness({auto:true,multi:true});
 try{
  const a=await f.admission('a'),b=await f.admission('b'),keyB=generateKeyPairSync('ed25519');
  assert.equal(JSON.parse(await a.list(actor(f.now,'a'))).workloads.length,0);
  assert.equal(await f.enrolled(),0);
  const wa=JSON.parse(await f.customer.mutate(actor(f.now,'a'),'same-request',create(f.client.publicKey))).workloads[0];
  const wb=JSON.parse(await f.customer.mutate(actor(f.now,'b'),'same-request',create(keyB.publicKey))).workloads[0];
  assert.equal(wa.status,'admitted');assert.equal(wb.status,'admitted');assert.notEqual(wa.id,wb.id);assert.equal(await f.enrolled(),2);
  const ca=await f.control('a'),cb=await f.control('b');await ca.deliver();await cb.deliver();
  assert.equal(JSON.parse(await a.list(actor(f.now,'a'))).workloads[0].status,'delivered');
  await f.setTime(f.now+361);
  for(const [tenant,service,w,key] of [['a',a,wa,f.client.privateKey],['b',b,wb,keyB.privateKey]]){
   assert.equal(JSON.parse(await service.list(actor(f.now,String(tenant)))).workloads[0].status,'ready');
   const token=await credential(await f.exchange('org-'+tenant,'ingress-a',['op'],w.id,key));
   assert.equal((await f.guard(token.access_token,f.signer.proof(token.access_token,'POST','https://guard.invalid/v1/guard',f.now),'ingress-b')).status,200);
  }
  const sa=JSON.parse(await ca.inspect()),sb=JSON.parse(await cb.inspect());
  assert.notEqual(sa.generation,sb.generation);assert.deepEqual(sa.snapshot.review.payload.rows.map(/** @param {{tenant:string}} r */ r=>r.tenant),['a']);
  assert.deepEqual(sb.snapshot.review.payload.rows.map(/** @param {{tenant:string}} r */ r=>r.tenant),['b']);
  await assert.rejects(b.list(actor(f.now,'a')));
  await assert.rejects(a.mutate(actor(f.now,'a'),'cross',JSON.stringify({operation:'revoke',id:wb.id,revision:wb.revision})));
  const forged=sa.snapshot.review.payload.rows.map(/** @param {import('../owner/control/protocol.js').Row} r */ r=>({...r,tenant:'b'}));
  await assert.rejects(ca.submit(await f.command({profile:sa.snapshot.review.payload.profile,rows:forged},'a')));
  const activity=await ca.activity()+await cb.activity();
  await (await f.ingress('renewal-job')).fetch('https://job.invalid/run');
  assert.equal(await f.enrolled(),2);assert.ok(await ca.activity()+await cb.activity()>activity);
  assert.deepEqual(JSON.parse(await ca.inspect()).snapshot.review.payload.rows,sa.snapshot.review.payload.rows);
  await f.restart();assert.equal(JSON.parse(await (await f.admission('b')).list(actor(f.now,'b'))).workloads[0].status,'ready');
 }finally{await f.close();}
});
test('private input, wrong namespace and disabled features cannot provision an organization',async()=>{
 const f=await ownerHarness({auto:true,multi:true});
 try{
  const a=await f.admission('a'),body=JSON.parse(create(f.client.publicKey));body.jwk.d='PRIVATE-NEVER-PERSIST';
  await assert.rejects(a.mutate(actor(f.now,'a'),'private',JSON.stringify(body)));
  await assert.rejects(a.mutate(actor(f.now,'b'),'wrong-namespace',create(f.client.publicKey)));
  await a.setFeatureEnabled(false);await assert.rejects(a.mutate(actor(f.now,'a'),'disabled',create(f.client.publicKey)));
  assert.equal(await a.stored(),'[]');assert.equal(await f.enrolled(),0);
  for(const path of ['/v1/token','/v1/guard'])assert.ok((await (await f.ingress()).fetch('https://guard.invalid'+path,{method:'POST',headers:{cookie:'dashboard=session'},body:'{}'})).status>=400);
 }finally{await f.close();}
});
test('directory loss cannot adopt an existing organization authority',async()=>{
 const f=await ownerHarness({auto:true,multi:true});
 try{
  const a=await f.admission('a');await a.mutate(actor(f.now,'a'),'create',create(f.client.publicKey));
  await f.eraseDirectory();await assert.rejects(a.mutate(actor(f.now,'a'),'after-loss',create(f.client.publicKey)));
  assert.equal(JSON.parse(await (await f.control('a')).inspect()).revision,1);
 }finally{await f.close();}
});
test('an unapproved connection cannot open a profile and a missing read catalog cannot admit',async()=>{
 const f=await ownerHarness({auto:true,multi:true});
 try{
  const s=await f.admission('a');
  await assert.rejects(s.mutate(actor(f.now,'a'),'wrong-connection',JSON.stringify({...JSON.parse(create(f.client.publicKey)),connection:'other'})));
  assert.equal(await f.enrolled(),0);
  await s.configureCatalog('');await assert.rejects(s.mutate(actor(f.now,'a'),'missing-catalog',create(f.client.publicKey)));
  assert.equal(await f.enrolled(),0);
 }finally{await f.close();}
});
