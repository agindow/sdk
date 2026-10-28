import { test } from 'node:test';
import assert from 'node:assert/strict';
import { ownerHarness } from './support/owner-harness.mjs';

test('refresh and restart retain assertion fences through expiry plus skew', async () => {
  const f = await ownerHarness();
  try {
    const owner = await f.owner();
    await owner.retained();
    await f.setTime(f.now + 361);
    const issuedAt = f.now;
    assert.equal((await f.exchange('retained')).status, 200);
    const config = JSON.parse(f.registrations);
    config.not_after--;
    await owner.configure(JSON.stringify(config));
    await f.setTime(issuedAt + 149);
    assert.equal((await f.exchange('retained')).status, 401);
    assert.equal(await owner.retained(), 1);
    await f.restart();
    assert.equal((await f.exchange('retained')).status, 401);
    // At exp + skew the original assertion is expired; it cannot mint again even after cleanup.
    await f.setTime(issuedAt + 150);
    assert.equal((await f.exchange('retained', 'ingress-b', ['op'], 'workload', f.client.privateKey, issuedAt)).status, 401);
    assert.equal((await f.exchange('fresh')).status, 200);
    assert.equal(await (await f.owner()).retained(), 1);
    await f.setTime(issuedAt + 149);
    assert.equal((await f.exchange('rollback')).status, 503);
    await f.restart();
    assert.equal((await f.exchange('rollback-after-restart')).status, 503);
  } finally { await f.close(); }
});

test('future assertion issuance never survives the full empty-store quarantine', async () => {
  const f = await ownerHarness();
  try {
    await (await f.owner()).retained();
    await f.setTime(f.now + 361);
    // Native acceptance permits +30 seconds future issuance, and rejects +31.
    assert.equal((await f.exchange('future', 'ingress-a', ['op'], 'workload', f.client.privateKey, f.now + 31)).status, 401);
    const issuedAt = f.now + 30;
    assert.equal((await f.exchange('future', 'ingress-a', ['op'], 'workload', f.client.privateKey, issuedAt, 301)).status, 401);
    assert.equal((await f.exchange('future', 'ingress-a', ['op'], 'workload', f.client.privateKey, issuedAt, 300)).status, 200);
    await (await f.owner()).eraseLedger();
    await f.restart();
    assert.equal((await f.exchange('future')).status, 503);
    await f.setTime(f.now + 359);
    assert.equal((await f.exchange('fresh')).status, 503);
    await f.setTime(f.now + 1);
    assert.equal((await f.exchange('future', 'ingress-b', ['op'], 'workload', f.client.privateKey, issuedAt, 300)).status, 401);
    assert.equal((await f.exchange('fresh')).status, 200);
  } finally { await f.close(); }
});
