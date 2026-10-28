import { test } from 'node:test';
import assert from 'node:assert/strict';
import { ownerHarness, credential } from './support/owner-harness.mjs';
const endpoint='https://guard.invalid/v1/guard';
test('invalid signed commands cannot consume a valid receipt or widen authority',async()=>{
  const f=await ownerHarness({auto:true});
  try {
    const c=await f.control(), command=await f.command();
    const broken=JSON.parse(command); broken.signature=broken.signature.slice(0,-4)+'AAAA';
    await assert.rejects(c.submit(JSON.stringify(broken)));
    assert.equal(JSON.parse(await c.inspect()).revision,0);
    for(const overrides of [
      {profile:'foreign'}, {origin:'https://foreign.invalid'}, {iat:f.now+31},
      {exp:f.now}, {exp:f.now+301}, {reviewUntil:f.now+2592001},
    ]) await assert.rejects(c.submit(await f.command(overrides)));
    await c.submit(command); await c.deliver();
    const duplicate=JSON.parse(await c.submit(command)); assert.equal(duplicate.revision,1);
    await c.fillCommands(f.now+300);
    await assert.rejects(c.submit(await f.command({action:'stop'})));
    assert.equal(JSON.parse(await c.inspect()).revision,1);
  }finally{await f.close();}
});
test('snapshot capability, sequence and generation fencing survive restart',async()=>{
  const f=await ownerHarness({auto:true});
  try {
    const c=await f.control();
    await c.submit(await f.command()); await c.deliver();
    const old=JSON.parse(await c.inspect()).snapshot, owner=await f.owner();
    await assert.rejects(owner.installPolicy(JSON.stringify(old),'wrong','workload'));
    await f.setTime(f.now+600); await c.renewCurrent();
    const current=JSON.parse(await c.inspect()).snapshot;
    await assert.rejects(owner.installPolicy(JSON.stringify(old),f.deliveryCapability,'workload'));
    assert.equal(await owner.installPolicy(JSON.stringify(current),f.deliveryCapability,'workload'),`${current.review.payload.generation}:1:2`);
    const token=await credential(await f.exchange('before-reset'));
    await f.restart();
    assert.equal((await f.exchange('before-reset')).status,401);
    assert.equal((await f.guard(token.access_token,f.signer.proof(token.access_token,'POST',endpoint,f.now),'ingress-a')).status,503);
    await assert.rejects((await f.owner()).installPolicy(JSON.stringify(old),f.deliveryCapability,'workload'));
    const generation=JSON.parse(await (await f.control()).inspect()).generation;
    await (await f.control()).eraseControl(); await f.restart();
    const fresh=await f.control();
    await fresh.submit(await f.command()); await fresh.deliver();
    // A new generation without explicit previous-generation approval cannot replace an owner floor.
    assert.ok(JSON.parse(await fresh.inspect()).pending.length>0);
    await fresh.submit(await f.command({previousGeneration:generation})); await fresh.deliver();
    assert.equal(JSON.parse(await fresh.inspect()).pending.length,0);
  }finally{await f.close();}
});
test('a changed reviewed row needs admission and manual rollback cannot override auto policy',async()=>{
  const f=await ownerHarness({auto:true});
  try {
    const c=await f.control(); await c.submit(await f.command()); await c.deliver();
    await f.setTime(f.now+361);
    const rows=/** @type {{registrations:import('../owner/control/protocol.js').Row[]}} */ (JSON.parse(f.registrations)).registrations.map(r=>r.id==='workload'?{...r,tools:['op']}:r);
    await assert.rejects(c.submit(await f.command({action:'resume',rows})));
    await c.submit(await f.command({action:'admit',rows})); await c.deliver();
    assert.equal((await f.exchange('removed-tool','ingress-a',['recovery'])).status,403);
    const issued=await f.exchange('valid-tool'); assert.equal(issued.status,200);
    const token=await credential(issued), owner=await f.owner();
    await owner.setMode('manual'); assert.equal((await f.exchange('manual-rollback')).status,503);
    await owner.setMode('auto'); await owner.setEnabled('false');
    assert.equal((await f.guard(token.access_token,f.signer.proof(token.access_token,'POST',endpoint,f.now),'ingress-b')).status,503);
  }finally{await f.close();}
});

test('reviewer rotation requires a newly signed admission and rejects removed trust',async()=>{
  const f=await ownerHarness({auto:true});
  try {
    const c=await f.control(); await c.submit(await f.command()); await c.deliver();
    const dual=JSON.stringify([...f.reviewKeys,f.nextReviewKey]);
    await c.configureKeys(dual);
    for(const id of ['workload','foreign']) await (await f.owner(id)).configureKeys(dual);
    await c.submit(await f.command({kid:'reviewer-next'})); await c.deliver();
    const old=await f.command({action:'stop'}), nextOnly=JSON.stringify([f.nextReviewKey]);
    await c.configureKeys(nextOnly);
    for(const id of ['workload','foreign']) await (await f.owner(id)).configureKeys(nextOnly);
    await assert.rejects(c.submit(old));
    const current=JSON.parse(await c.renewCurrent()); assert.equal(current.revision,2);
    await f.setTime(f.now+361);
    assert.equal((await f.exchange('rotated-reviewer')).status,200);
  }finally{await f.close();}
});

test('delegation obeys the current reviewed ceiling before consuming the parent proof',async()=>{
  const f=await ownerHarness({auto:true});
  try {
    const c=await f.control(); await c.submit(await f.command()); await c.deliver();
    await f.setTime(f.now+361);
    const parent=await credential(await f.exchange('parent','ingress-a',['op','recovery']));
    const rows=/** @type {{registrations:import('../owner/control/protocol.js').Row[]}} */ (JSON.parse(f.registrations)).registrations.map(r=>r.id==='workload'?{...r,tools:['op']}:r);
    await c.submit(await f.command({rows})); await c.deliver();
    const target='https://guard.invalid/v1/token', proof=f.signer.proof(parent.access_token,'POST',target,f.now);
    /** @param {string[]} tools */
    const delegate=async tools=>(await f.ingress()).fetch(target,{method:'POST',headers:{dpop:proof},body:JSON.stringify({
      grant_type:'urn:ietf:params:oauth:grant-type:token-exchange',subject_token_type:'urn:ietf:params:oauth:token-type:access_token',
      subject_token:parent.access_token,connection:'c',subject:'child',audience:'agindow:guard',tools,ttl:60,cnf:{jkt:f.signer.thumbprint},
    })});
    assert.equal((await delegate(['recovery'])).status,403);
    assert.equal((await delegate(['op'])).status,200);
    assert.equal((await delegate(['op'])).status,401);
    const parts=parent.access_token.split('.'), malformed=[parts[0],'bm90LWpzb24',parts[2]].join('.');
    assert.equal((await f.guard(malformed,f.signer.proof(malformed,'POST',endpoint,f.now),'ingress-b')).status,401);
    assert.equal(f.dispatches,0);
  }finally{await f.close();}
});
