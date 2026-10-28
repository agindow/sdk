import { test } from 'node:test';
import assert from 'node:assert/strict';
import { loadRenewal } from './support/load-renewal.mjs';
test('review transitions use generation, CAS, exact resume rows and monotonic time', async () => {
  const { transition, initial } = /** @type {typeof import('../owner/control/state.js')} */ (await loadRenewal('state'));
  const { reviewed } = /** @type {typeof import('../owner/control/protocol.js')} */ (await loadRenewal('protocol'));
  const row = {id:'a',kid:'k',x:'a'.repeat(43),audience:'https://guard.invalid/token',tenant:'t',environment:'test',connection:'c',tools:['read']};
  const content = await reviewed('https://guard.invalid','primary',[row]);
  const payload = {version:1,origin:'https://guard.invalid',profile:'primary',generation:'generation',previousGeneration:null,expectedRevision:0,id:'one',kid:'review',iat:1000,exp:1200,action:'admit',reviewUntil:10000,...content};
  const signed = /** @type {import('../owner/control/protocol.js').SignedCommand} */ ({payload,signature:'fixture'});
  const active = await transition(initial('generation',1000),signed,1000);
  assert.equal(active.snapshot?.notAfter,4600);
  await assert.rejects(transition(active,signed,1001));
  await assert.rejects(transition(initial('replacement',1000),signed,1000));
  await assert.rejects(transition(active,{...signed,payload:{...signed.payload,expectedRevision:1,action:'resume',digest:'forged'}},1001));
  const stopped = await transition(active,{...signed,payload:{...signed.payload,expectedRevision:1,action:'stop'}},1001);
  assert.equal(stopped.snapshot?.status,'stopped');
  await assert.rejects(transition(stopped,{...signed,payload:{...signed.payload,expectedRevision:2,action:'resume'}},999));
});
