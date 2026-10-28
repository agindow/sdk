import {test} from 'node:test';
import assert from 'node:assert/strict';
import {ownerHarness,credential} from './support/owner-harness.mjs';
/** @param {number} now */
const actor=now=>JSON.stringify({actor:'owner',tenant:'a',environment:'test',role:'owner',authenticatedAt:now,authorizedAt:now});
/** @param {Awaited<ReturnType<typeof ownerHarness>>} f */
const create=f=>JSON.stringify({operation:'create',connection:'c',jwk:{kty:'OKP',crv:'Ed25519',kid:'client-key',x:f.client.publicKey.export({format:'jwk'}).x}});
test('customer intent signs, commits, delivers and becomes exchange ready without operator admit',async()=>{
 const f=await ownerHarness({auto:true});
 try{
  await f.setupAdmission();const service=await f.admission(),c=await f.control();
  let view=JSON.parse(await service.mutate(actor(f.now),'request-one',create(f)));
  assert.equal(view.workloads.length,1);assert.equal(view.workloads[0].status,'admitted');
  const id=view.workloads[0].id;
  await c.deliver();
  view=JSON.parse(await service.list(actor(f.now)));assert.equal(view.workloads[0].status,'delivered');
  await f.setTime(f.now+361);await service.setTime(f.now);
  view=JSON.parse(await service.list(actor(f.now)));assert.equal(view.workloads[0].status,'ready');
  const token=await credential(await f.exchange('self-serve','ingress-a',['op'],id));
  assert.equal((await f.guard(token.access_token,f.signer.proof(token.access_token,'POST','https://guard.invalid/v1/guard',f.now),'ingress-a')).status,200);
  const again=JSON.parse(await service.mutate(actor(f.now),'request-one',create(f)));
  assert.equal(again.workloads.length,1);assert.equal(JSON.parse(await c.inspect()).revision,1);
  await f.restart();
  assert.equal(JSON.parse(await (await f.admission()).list(actor(f.now))).workloads[0].status,'ready');
 }finally{await f.close();}
});
test('private keys, other tenants, stale sessions and member roles create no durable intents',async()=>{
 const f=await ownerHarness({auto:true});
 try{
  await f.setupAdmission();const s=await f.admission();
  const valid=JSON.parse(actor(f.now));
  for(const change of [{tenant:'b'},{role:'member'},{authenticatedAt:f.now-301},{authorizedAt:f.now-31}])await assert.rejects(s.mutate(JSON.stringify({...valid,...change}),'bad',create(f)));
  const body=JSON.parse(create(f));body.jwk.d='PRIVATE-SECRET';await assert.rejects(s.mutate(actor(f.now),'private',JSON.stringify(body)));
  assert.equal(await s.stored(),'[]');assert.equal(JSON.parse(await (await f.control()).inspect()).revision,0);
  for(const route of ['/v1/token','/v1/guard'])assert.ok((await (await f.ingress()).fetch('https://guard.invalid'+route,{method:'POST',headers:{cookie:'dashboard=session'},body:'{}'})).status>=400);
  assert.equal(f.dispatches,0);
 }finally{await f.close();}
});
test('rotation and revocation require current revision; replay cannot resurrect a removed grant',async()=>{
 const f=await ownerHarness({auto:true});
 try{
  await f.setupAdmission();const s=await f.admission(),c=await f.control();
  let w=JSON.parse(await s.mutate(actor(f.now),'create',create(f))).workloads[0];
  const rotated={operation:'rotate',id:w.id,revision:w.revision,jwk:{...JSON.parse(create(f)).jwk,kid:'next'}};
  w=JSON.parse(await s.mutate(actor(f.now),'rotate',JSON.stringify(rotated))).workloads[0];assert.equal(w.kid,'next');assert.equal(w.revision,2);
  await assert.rejects(s.mutate(actor(f.now),'stale',JSON.stringify(rotated)));
  w=JSON.parse(await s.mutate(actor(f.now),'revoke',JSON.stringify({operation:'revoke',id:w.id,revision:w.revision}))).workloads[0];
  assert.equal(w.status,'revoked');assert.equal(JSON.parse(await c.inspect()).snapshot.status,'revoked');
  await s.mutate(actor(f.now),'create',create(f));assert.equal(JSON.parse(await c.inspect()).revision,3);
  await assert.rejects(s.mutate(actor(f.now),'create',JSON.stringify({...JSON.parse(create(f)),connection:'other'})));
 }finally{await f.close();}
});
test('a valid platform signature cannot bootstrap two rows or resume stopped policy',async()=>{
 const f=await ownerHarness({auto:true});
 try{
  const p=await f.setupAdmission(),s=await f.admission(),c=await f.control();
  const x=f.client.publicKey.export({format:'jwk'}).x;if(!x)throw Error('public key');
  /** @type {import('../owner/control/protocol.js').Row} */
  const row={id:'workload-one',kid:'client-key',x,tenant:'a',environment:'test',connection:'c',tools:['op'],audience:p.audience};
  await assert.rejects(c.submit(await f.command({kid:'reviewer-next',rows:[row,{...row,id:'workload-two'}],reviewUntil:p.until})));
  await s.mutate(actor(f.now),'create',create(f));
  const rows=JSON.parse(await c.inspect()).snapshot.review.payload.rows;
  await c.submit(await f.command({action:'stop',rows}));
  await assert.rejects(c.submit(await f.command({kid:'reviewer-next',action:'resume',rows,reviewUntil:p.until})));
  await assert.rejects(c.submit(await f.command({kid:'reviewer-next',action:'admit',rows,reviewUntil:p.until})));
  assert.equal(JSON.parse(await c.inspect()).snapshot.status,'stopped');
 }finally{await f.close();}
});
test('durable quota, lost acknowledgements and pending delivery remain bounded across restart',async()=>{
 const f=await ownerHarness({auto:true});
 try{
  await f.setupAdmission();let s=await f.admission();
  await (await f.control()).loseNextAck();
  for(let i=0;i<10;i++)await s.mutate(actor(f.now),'create-'+i,create(f));
  await assert.rejects(s.mutate(actor(f.now),'overflow',create(f)));
  await f.restart();s=await f.admission();
  await assert.rejects(s.mutate(actor(f.now),'overflow-after-restart',create(f)));
  const view=JSON.parse(await s.mutate(actor(f.now),'create-0',create(f)));
  assert.equal(view.workloads.length,10);
  assert.ok(view.workloads.every(/** @param {{status:string}} w */ w=>w.status!=='ready'));
  assert.equal(JSON.parse(await (await f.control()).inspect()).revision,10);
 }finally{await f.close();}
});
test('lost admission storage cannot silently bootstrap over an active authority',async()=>{
 const f=await ownerHarness({auto:true});
 try{
  await f.setupAdmission();const s=await f.admission();
  await s.mutate(actor(f.now),'original',create(f));await s.eraseAdmission();
  await assert.rejects(s.mutate(actor(f.now),'after-loss',create(f)));
  assert.equal(JSON.parse(await (await f.control()).inspect()).revision,1);
 }finally{await f.close();}
});
test('reconfiguring a profile for another tenant cannot expose prior workload metadata',async()=>{
 const f=await ownerHarness({auto:true});
 try{
  const p=await f.setupAdmission(),s=await f.admission();await s.mutate(actor(f.now),'original',create(f));
  await s.configurePolicy(JSON.stringify({...p,version:'p2',tenant:'b'}));
  await assert.rejects(s.list(JSON.stringify({...JSON.parse(actor(f.now)),tenant:'b'})));
  await s.configurePolicy(JSON.stringify({...p,tools:['other']}));
  await assert.rejects(s.list(actor(f.now)));
 }finally{await f.close();}
});
test('completed receipts keep compact signed provenance rather than repeated full-profile commands',async()=>{
 const f=await ownerHarness({auto:true});
 try{
  await f.setupAdmission();const s=await f.admission();await s.mutate(actor(f.now),'compact',create(f));
  const records=JSON.parse(await s.stored()).map(/** @param {{body:string}} row */ row=>JSON.parse(row.body));
  assert.equal(records[0].command===null,true);assert.equal(typeof records[0].provenance.signature,'string');
  assert.ok(records[0].provenance.encoded.includes('agindow-review-command-v1'));
 }finally{await f.close();}
});
test('unsigned intents expire instead of gaining admission after an extended signer outage',async()=>{
 const f=await ownerHarness({auto:true});
 try{
  await f.setupAdmission();const s=await f.admission();await s.setSigningEnabled(false);
  const submitted=JSON.parse(await s.mutate(actor(f.now),'pending',create(f)));assert.equal(submitted.requests[0].status,'submitted');
  await f.setTime(f.now+301);await s.setTime(f.now);await s.setSigningEnabled(true);await s.runAlarm();
  const result=JSON.parse(await s.list(actor(f.now)));assert.equal(result.requests[0]?.status,'rejected');
  assert.equal(JSON.parse(await (await f.control()).inspect()).revision,0);
 }finally{await f.close();}
});
test('disabled admission feature refuses before persisting or signing an intent',async()=>{
 const f=await ownerHarness({auto:true});
 try{
  await f.setupAdmission();const s=await f.admission();await s.setFeatureEnabled(false);
  await assert.rejects(s.mutate(actor(f.now),'disabled',create(f)));
  assert.equal(await s.stored(),'[]');assert.equal(JSON.parse(await (await f.control()).inspect()).revision,0);
 }finally{await f.close();}
});
test('a concurrent reviewed admit rebases safely while a concurrent revoke defeats a stale rotation',async()=>{
 const f=await ownerHarness({auto:true});
 try{
  const p=await f.setupAdmission(),s=await f.admission(),c=await f.control();
  const x=f.client.publicKey.export({format:'jwk'}).x;if(!x)throw Error('public key');
  /** @type {import('../owner/control/protocol.js').Row} */
  const operator={id:'workload-operator',kid:'client-key',x,tenant:'a',environment:'test',connection:'c',audience:p.audience,tools:['op']};
  await c.competeOnce(await f.command({rows:[operator],reviewUntil:p.until}));
  const view=JSON.parse(await s.mutate(actor(f.now),'rebase',create(f))),w=view.workloads[0];
  const state=JSON.parse(await c.inspect());assert.equal(state.revision,2);assert.equal(state.snapshot.review.payload.rows.length,2);
  assert.deepEqual(state.snapshot.review.payload.rows.find(/** @param {{id:string}} r */ r=>r.id===operator.id),operator);
  await c.competeOnce(await f.command({action:'revoke',rows:state.snapshot.review.payload.rows,reviewUntil:p.until}));
  const result=JSON.parse(await s.mutate(actor(f.now),'rotate-race',JSON.stringify({operation:'rotate',id:w.id,revision:w.revision,jwk:{kty:'OKP',crv:'Ed25519',kid:'next',x}})));
  assert.equal(JSON.parse(await c.inspect()).snapshot.status,'revoked');
  assert.ok(result.workloads.every(/** @param {{status:string}} row */ row=>row.status!=='ready'));
  assert.equal(result.requests.find(/** @param {{id:string}} r */ r=>r.id==='rotate-race')?.status,'rejected');
 }finally{await f.close();}
});
test('recovers the immutable command receipt when a lost acknowledgement is followed by another review',async()=>{
 const f=await ownerHarness({auto:true});
 try{
  const p=await f.setupAdmission(),s=await f.admission(),c=await f.control();
  const first=JSON.parse(await s.mutate(actor(f.now),'create',create(f))).workloads[0];
  const rows=JSON.parse(await c.inspect()).snapshot.review.payload.rows.map(/** @param {import('../owner/control/protocol.js').Row} row */ row=>({...row,kid:'next'}));
  await c.advanceAfterCommit(await f.command({rows,expectedRevision:2,reviewUntil:p.until}));await c.loseNextAck();
  const result=JSON.parse(await s.mutate(actor(f.now),'lost-rotation',JSON.stringify({operation:'rotate',id:first.id,revision:first.revision,jwk:{...JSON.parse(create(f)).jwk,kid:'next'}})));
  assert.equal(JSON.parse(await c.inspect()).revision,3);assert.equal(result.workloads[0]?.kid,'next');
  assert.equal(result.workloads[0]?.revision,2);assert.equal(result.requests.length,0);
 }finally{await f.close();}
});
