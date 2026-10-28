import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { loadRenewal } from './support/load-renewal.mjs';
const protocol = () => loadRenewal('protocol');
/** @type {import('../owner/control/protocol.js').Row} */
const row = { id:'a', kid:'k', x:'a'.repeat(43), audience:'https://guard.invalid/v1/token', tenant:'t', environment:'test', connection:'c', tools:['read','list'] };
test('review digest is order independent but commits every authority field', async () => {
  const { reviewed } = /** @type {typeof import('../owner/control/protocol.js')} */ (await protocol());
  const base = await reviewed('https://guard.invalid', 'primary', [row]);
  assert.equal((await reviewed('https://guard.invalid','primary',[{...row,tools:['list','read']}])).digest, base.digest);
  for (const field of /** @type {const} */ (['id','kid','x','audience','tenant','environment','connection'])) {
    const changed = { ...row, [field]: field==='x' ? 'b'.repeat(43) : field==='environment' ? 'live' : `${row[field]}2` };
    assert.notEqual((await reviewed('https://guard.invalid','primary',[changed])).digest, base.digest);
  }
  assert.notEqual((await reviewed('https://guard.invalid','other',[row])).digest, base.digest);
  assert.notEqual((await reviewed('https://guard.invalid','primary',[{...row,id:'café'}])).digest,(await reviewed('https://guard.invalid','primary',[{...row,id:'cafe\u0301'}])).digest);
  const unicode=await reviewed('https://guard.invalid','primary',[{...row,tools:['😀','\uE000']}]);
  const tuple=[row.id,row.kid,row.x,row.audience,row.tenant,row.environment,row.connection,['\uE000','😀']];
  assert.equal(unicode.digest,createHash('sha256').update(JSON.stringify(['agindow-keyless-review-v1','https://guard.invalid','primary',[tuple]])).digest('hex'));
  await assert.rejects(reviewed('https://guard.invalid','primary',[row,row]));
  await assert.rejects(reviewed('https://guard.invalid','primary',[{...row,tools:['read','read']}]));
});
test('renewal cannot widen, resurrect, or extend an expired review', async () => {
  const { lease } = /** @type {typeof import('../owner/control/protocol.js')} */ (await protocol());
  const { reviewed } = /** @type {typeof import('../owner/control/protocol.js')} */ (await protocol());
  const review = await reviewed('https://guard.invalid', 'primary', [row]);
  /** @type {import('../owner/control/protocol.js').LeaseState} */
  const state = { status:'active', until:5000, notAfter:2000, issuedAt:1000, sequence:1, digest:review.digest, rows:[row] };
  assert.equal((await lease(state,1600,'https://guard.invalid','primary')).notAfter,5000);
  await assert.rejects(lease({...state,rows:[{...row,tenant:'other'}]},1600,'https://guard.invalid','primary'));
  await assert.rejects(lease(state,2000,'https://guard.invalid','primary'));
  await assert.rejects(lease({...state,status:'revoked'},1600,'https://guard.invalid','primary'));
});

test('the reviewed set has a UTF-8 byte bound even for non-ASCII literals',async()=>{
  const {reviewed}=/** @type {typeof import('../owner/control/protocol.js')} */ (await protocol());
  const tools=Array.from({length:64},(_,i)=>'😀'.repeat(120)+i);
  const rows=Array.from({length:9},(_,i)=>({...row,id:'unicode-'+i,tools}));
  await assert.rejects(reviewed('https://guard.invalid','primary',rows));
});

test('delivery planning drains healthy backlog promptly and preserves fairness across renewals',async()=>{
  const {deliveryPlan,renewalTargets}=/** @type {typeof import('../owner/control/delivery-plan.js')} */ (await loadRenewal('delivery-plan'));
  const targets=Array.from({length:256},(_,i)=>`owner-${i}`);
  const pending=deliveryPlan(targets,[],1000);
  assert.equal(pending.delay,1); assert.equal(pending.batch.length,16);
  const retries=targets.slice(0,16).map(id=>({id,at:1060}));
  assert.deepEqual(deliveryPlan(targets,retries,1000).batch,targets.slice(16,32));
  assert.equal(deliveryPlan(targets.slice(0,16),retries,1000).delay,60);
  assert.equal(renewalTargets(['owner-255'],targets)[0],'owner-255');
});
