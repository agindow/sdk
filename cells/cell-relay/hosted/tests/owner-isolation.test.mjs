import { test } from 'node:test';
import assert from 'node:assert/strict';
import { ownerHarness, credential } from './support/owner-harness.mjs';

test('a saturated workload cannot exhaust another tenant proof fences', async () => {
  const f = await ownerHarness();
  try {
    await (await f.owner()).retained();
    await (await f.owner('foreign')).retained();
    await f.setTime(f.now + 361);
    const issue = await f.exchange('saturate');
    assert.equal(issue.status, 200);
    const { access_token: access } = await credential(issue);
    for (let i = 0; i < 1024; i++) {
      if (i === 900) await f.setTime(f.now + 61);
      const proof = f.signer.proof(access, 'POST', 'https://guard.invalid/v1/guard', f.now);
      const response = await f.guard(access, proof, 'ingress-a');
      assert.equal(response.status, 200, `admission ${i}: ${await response.text()}`);
    }
    const full = await f.guard(access, f.signer.proof(access, 'POST', 'https://guard.invalid/v1/guard', f.now), 'ingress-b');
    assert.equal(full.status, 429);
    const foreign = await f.exchange('saturate', 'ingress-b', ['op'], 'foreign');
    assert.equal(foreign.status, 200);
    const { access_token: other } = await credential(foreign);
    const response = await f.guard(other, f.signer.proof(other, 'POST', 'https://guard.invalid/v1/guard', f.now), 'ingress-b', 'd');
    assert.equal(response.status, 200, await response.text());
    assert.equal(f.dispatches, 1025);
  } finally { await f.close(); }
});

test('invalid scope and signature cannot burn a valid workload proof', async () => {
  const f = await ownerHarness();
  try {
    await (await f.owner()).retained();
    await f.setTime(f.now + 361);
    const { access_token: access } = await credential(await f.exchange('binding'));
    const proof = f.signer.proof(access, 'POST', 'https://guard.invalid/v1/guard', f.now);
    const parts = proof.split('.');
    assert.ok(parts[2]);
    const signature = Buffer.from(parts[2], 'base64url');
    const first = signature[0];
    assert.notEqual(first, undefined);
    if (first === undefined) throw Error('empty signature');
    signature[0] = first ^ 1;
    const corrupted = [parts[0], parts[1], signature.toString('base64url')].join('.');
    assert.equal((await f.guard(access, corrupted, 'ingress-a')).status, 401);
    assert.equal((await f.guard(access, proof, 'ingress-b', 'd')).status, 403);
    assert.equal(f.dispatches, 0);
    assert.equal((await f.guard(access, proof, 'ingress-b')).status, 200);
    assert.equal((await f.guard(access, proof, 'ingress-a')).status, 401);
    assert.equal(f.dispatches, 1);
  } finally { await f.close(); }
});
