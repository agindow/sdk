import {test} from 'node:test';
import assert from 'node:assert/strict';
import {ownerHarness} from './support/owner-harness.mjs';
/** @param {number} now @param {string} tenant */
const actor=(now,tenant)=>JSON.stringify({actor:'owner',tenant,environment:'test',role:'owner',authenticatedAt:now,authorizedAt:now});
/** @param {Awaited<ReturnType<typeof ownerHarness>>} f */
const create=f=>JSON.stringify({operation:'create',connection:'c',jwk:{kty:'OKP',crv:'Ed25519',kid:'client-key',x:f.client.publicKey.export({format:'jwk'}).x}});
test('renewal visits only enrolled organizations in bounded batches and finishes its backlog',async()=>{
 const f=await ownerHarness({auto:true,multi:true});
 try{
  const directory=await f.directory();
  await (await f.ingress('renewal-job')).fetch('https://job.invalid/run');assert.equal(await f.enrolled(),0);
  const controls=[];
  for(let i=0;i<17;i++){
   const tenant='org-'+i;await f.customer.mutate(actor(f.now,tenant),'create',create(f));controls.push(await f.control(tenant));
  }
  const before=await Promise.all(controls.map(c=>c.activity()));
  await (await f.ingress('renewal-job')).fetch('https://job.invalid/run');
  const middle=await Promise.all(controls.map(c=>c.activity()));
  assert.equal(middle.filter((n,i)=>n!==before[i]).length,16);
  await directory.runAlarm();
  const after=await Promise.all(controls.map(c=>c.activity()));assert.equal(after.filter((n,i)=>n!==before[i]).length,17);
  assert.equal(await f.enrolled(),17);
  assert.equal((await(await f.ingress('renewal-job')).fetch('https://job.invalid/write')).status,403);
 }finally{await f.close();}
});
test('per-org quotas do not consume another organization quota and cross-org delivery is rejected',async()=>{
 const f=await ownerHarness({auto:true,multi:true});
 try{
  const a=await f.admission('a'),b=await f.admission('b');
  for(let i=0;i<10;i++)await a.mutate(actor(f.now,'a'),'a-'+i,create(f));
  await assert.rejects(a.mutate(actor(f.now,'a'),'limit',create(f)));
  const wb=JSON.parse(await b.mutate(actor(f.now,'b'),'b',create(f))).workloads[0];
  const sa=JSON.parse(await(await f.control('a')).inspect());
  await assert.rejects((await f.owner(wb.id)).installPolicy(JSON.stringify(sa.snapshot),f.deliveryCapability,wb.id));
  assert.equal(JSON.parse(await b.list(actor(f.now,'b'))).workloads.length,1);
 }finally{await f.close();}
});
test('authority generation loss and admission-store loss refuse automatic baseline import',async()=>{
 const f=await ownerHarness({auto:true,multi:true});
 try{
  const a=await f.admission('a');await a.mutate(actor(f.now,'a'),'a',create(f));await a.eraseAdmission();
  await assert.rejects(a.mutate(actor(f.now,'a'),'import',create(f)));
  await (await f.control('a')).eraseControl();await f.restart();
  await assert.rejects((await f.admission('a')).list(actor(f.now,'a')));
  await assert.rejects((await f.admission('a')).mutate(actor(f.now,'a'),'new-generation',create(f)));
 }finally{await f.close();}
});
