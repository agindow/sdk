import { test } from 'node:test';
import assert from 'node:assert/strict';
import { ownerHarness, credential } from './support/owner-harness.mjs';
const endpoint='https://guard.invalid/v1/guard';
test('private reviewed renewal extends unchanged rows and never enters guard dispatch', async()=>{
  const f=await ownerHarness({auto:true});
  try {
    const control=await f.control(), owner=await f.owner();
    await owner.retained();
    const review=await f.command();
    const receipt=JSON.parse(await control.submit(review));
    assert.equal(JSON.parse(await control.submit(review)).revision,receipt.revision);
    await control.deliver();
    const before=JSON.parse(await control.inspect());
    await f.setTime(f.now+361);
    const first=await f.exchange('first');
    assert.equal(first.status,200,await first.clone().text());
    await f.setTime(f.now+239);
    const original=await credential(first);
    const consumed=f.signer.proof(original.access_token,'POST',endpoint,f.now);
    assert.equal((await f.guard(original.access_token,consumed,'ingress-a')).status,200);
    const job=await f.ingress('renewal-job');
    const renewed=await job.fetch('https://fixture.invalid/run');
    assert.equal(renewed.status,200,await renewed.clone().text());
    const after=JSON.parse(await control.inspect());
    assert.equal((await f.guard(original.access_token,consumed,'ingress-b')).status,401);
    assert.equal(after.snapshot.notAfter,before.snapshot.notAfter+600);
    assert.deepEqual(after.snapshot.review.payload.rows,before.snapshot.review.payload.rows);
    assert.equal((await job.fetch('https://fixture.invalid/write')).status,403);
    const token=await credential(await f.exchange('last-before-expiry'));
    await f.setTime(after.snapshot.notAfter-1);
    const fresh=await credential(await f.exchange('fresh-before-expiry'));
    await f.setTime(after.snapshot.notAfter+1);
    assert.equal((await f.exchange('expired')).status,503);
    await control.eraseControl();
    await owner.eraseGuardSql();
    const activity=await control.activity();
    // Deliberately delete policy/replay SQL after warm-up: a guard can only use local validation.
    const response=await f.guard(fresh.access_token,f.signer.proof(fresh.access_token,'POST',endpoint,f.now),'ingress-b');
    assert.equal(response.status,200,await response.clone().text());
    assert.equal(await response.text(),' {"owner":true}\n');
    assert.equal(f.dispatches,2);
    assert.equal(await control.activity(),activity);
    assert.ok(token.access_token);
  } finally {await f.close();}
});

test('revocation, concurrent review and lost state cannot resume through the renewal capability',async()=>{
  const f=await ownerHarness({auto:true});
  try {
    const c=await f.control();
    const command=await f.command();
    await c.submit(command);
    await c.deliver();
    const old=JSON.parse(await c.inspect());
    const stop=await f.command({action:'stop'}), revoke=await f.command({action:'revoke'});
    const conflicts=await Promise.allSettled([c.submit(stop),c.submit(revoke)]);
    assert.equal(conflicts.filter(r=>r.status==='fulfilled').length,1);
    await assert.rejects(c.renewCurrent());
    await c.deliver();
    await f.setTime(f.now+361);
    assert.equal((await f.exchange('stopped')).status,503);
    const owner=await f.owner();
    await assert.rejects(owner.installPolicy(JSON.stringify(old.snapshot),f.deliveryCapability,'workload'));
    const previous=JSON.parse(await c.inspect()).generation;
    await c.eraseControl();
    await f.restart();
    const replacement=await f.control();
    assert.notEqual(JSON.parse(await replacement.inspect()).generation,previous);
    await assert.rejects(replacement.submit(command));
    await assert.rejects(replacement.renewCurrent());
    await replacement.submit(await f.command({previousGeneration:previous}));
    await replacement.deliver();
    assert.equal((await f.exchange('after-reviewed-recovery')).status,200);
  } finally {await f.close();}
});

test('drift, delivery partitions, clock rollback and lease expiry surface failure without widening',async()=>{
  const f=await ownerHarness({auto:true});
  try {
    const c=await f.control();
    await c.submit(await f.command()); await c.deliver();
    const clean=await c.inspect();
    await f.setTime(f.now+600);
    await c.corruptRows();
    await assert.rejects(c.renewCurrent());
    await c.restoreControl(clean);
    const owner=await f.owner(); await owner.deliveryDown(true);
    const renewal=JSON.parse(await c.renewCurrent());
    assert.ok(renewal.pending>0);
    await owner.deliveryDown(false); await f.setTime(f.now+60); await c.deliver();
    await c.renewCurrent();
    assert.equal(JSON.parse(await c.inspect()).pending.length,0);
    await f.setTime(f.now-1); await assert.rejects(c.renewCurrent());
    await f.setTime(renewal.notAfter); await assert.rejects(c.renewCurrent());
    assert.equal(JSON.parse(await c.inspect()).snapshot.status,'expired');
    assert.equal((await f.exchange('expired')).status,503);
    await assert.rejects(c.renewCurrent());
    await c.submit(await f.command({action:'resume'})); await c.deliver();
    assert.equal((await f.exchange('reviewed-resume')).status,200);
  } finally {await f.close();}
});

test('delivery alarms retry without replacing the explicit renewal job',async()=>{
  const f=await ownerHarness({auto:true});
  try {
    const c=await f.control(); await c.submit(await f.command());
    const before=JSON.parse(await c.inspect()).snapshot;
    await f.setTime(f.now+600); await c.runAlarm();
    const after=JSON.parse(await c.inspect()).snapshot;
    assert.equal(after.notAfter,before.notAfter);
    assert.equal(after.sequence,before.sequence);
  }finally{await f.close();}
});

test('bounded background delivery acknowledges only the owners reached in its batch',async()=>{
  const f=await ownerHarness({auto:true});
  try {
    const c=await f.control();
    const source=/** @type {{registrations:import('../owner/control/protocol.js').Row[]}} */ (JSON.parse(f.registrations)).registrations[0];
    assert.ok(source);
    const rows=Array.from({length:21},(_,i)=>({...source,id:`batch-${i}`}));
    await c.submit(await f.command({rows})); await c.deliver();
    assert.equal(JSON.parse(await c.inspect()).pending.length,5);
    await c.runAlarm();
    assert.equal(JSON.parse(await c.inspect()).pending.length,0);
  }finally{await f.close();}
});

test('a long healthy backlog drains before command expiry while failures keep a retry deadline',async()=>{
  const f=await ownerHarness({auto:true});
  try {
    const c=await f.control();
    const source=/** @type {{registrations:import('../owner/control/protocol.js').Row[]}} */ (JSON.parse(f.registrations)).registrations[0];
    assert.ok(source);
    const rows=Array.from({length:51},(_,i)=>({...source,id:`long-batch-${i}`}));
    await c.submit(await f.command({rows})); await c.deliver(); await c.runAlarm();
    assert.equal(JSON.parse(await c.inspect()).pending.length,19);
    assert.equal(await c.alarmAt(),(f.now+1)*1000);
    await c.runAlarm(); await c.runAlarm();
    assert.equal(JSON.parse(await c.inspect()).pending.length,0);
  }finally{await f.close();}
});
