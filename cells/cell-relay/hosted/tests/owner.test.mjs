import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { ownerHarness } from './support/owner-harness.mjs';
const initial = 1_800_000_000;
/** @param {object} value */
const b64 = (value) => Buffer.from(JSON.stringify(value)).toString('base64url');

test('independent ingress, owner restart and verifier reset never reopen replay admission', async () => {
  const fixture = await ownerHarness();
  const { owner, ingress, signer, exchange, guard, ownerName, fixturePort } = fixture;
  try {
    for (const authorization of ['Bearer malformed', 'DPoP malformed']) {
      const denied = await (
        await ingress()
      ).fetch('https://guard.invalid/v1/guard', { method: 'POST', headers: { authorization } });
      assert.equal(denied.status, 401);
    }
    const malformed = await (
      await ingress()
    ).fetch('https://guard.invalid/v1/token', { method: 'POST', body: '{' });
    assert.equal(malformed.status, 400);
    assert.equal((await exchange('warm')).status, 503);
    await fixture.setTime(initial + 361);
    const badAssertion = await (
      await ingress()
    ).fetch('https://guard.invalid/v1/token', {
      method: 'POST',
      body: JSON.stringify({ registration: 'workload', ttl: 300, subject_token: 'malformed' }),
    });
    assert.equal(badAssertion.status, 400);
    const issued = await exchange('first');
    assert.equal(issued.status, 200, await issued.clone().text());
    const access = /** @type {{access_token:string}} */ (await issued.json()).access_token;
    const proof = signer.proof(access, 'POST', 'https://guard.invalid/v1/guard', fixture.now);
    const raced = await Promise.all([
      guard(access, proof, 'ingress-a'),
      guard(access, proof, 'ingress-b'),
    ]);
    assert.deepEqual(raced.map((response) => response.status).sort(), [200, 401]);
    assert.equal(fixture.dispatches, 1);
    const assertionRace = await Promise.all([
      exchange('same-assertion'),
      exchange('same-assertion', 'ingress-b'),
    ]);
    assert.deepEqual(assertionRace.map((response) => response.status).sort(), [200, 401]);
    assert.equal((await exchange('excess', 'ingress-a', ['delete'])).status, 403);
    const delegationProof = signer.proof(access, 'POST', 'https://guard.invalid/v1/token', fixture.now);
    /** @param {string} name */
    const delegate = async (name) =>
      (await ingress(name)).fetch('https://guard.invalid/v1/token', {
        method: 'POST',
        headers: { dpop: delegationProof },
        body: JSON.stringify({
          grant_type: 'urn:ietf:params:oauth:grant-type:token-exchange',
          subject_token_type: 'urn:ietf:params:oauth:token-type:access_token',
          subject_token: access,
          connection: 'c',
          subject: 'child',
          audience: 'agindow:guard',
          tools: ['op'],
          ttl: 60,
          cnf: { jkt: signer.thumbprint },
        }),
      });
    const delegated = await Promise.all([delegate('ingress-a'), delegate('ingress-b')]);
    assert.deepEqual(delegated.map((response) => response.status).sort(), [200, 401]);
    const childResponse = delegated.find((response) => response.ok);
    assert.ok(childResponse);
    const child = /** @type {{access_token:string}} */ (await childResponse.json()).access_token;
    assert.equal(
      (
        await guard(
          child,
          signer.proof(child, 'POST', 'https://guard.invalid/v1/guard', fixture.now),
          'ingress-a',
        )
      ).status,
      200,
    );
    assert.equal(fixture.dispatches, 2);
    // A forged routing hint can locate another owner, but cannot authorize or consume its fences.
    const parts = access.split('.');
    assert.ok(parts[0]);
    const header = /** @type {{kid:string}} */ (
      JSON.parse(Buffer.from(parts[0], 'base64url').toString())
    );
    const foreignOwner = createHash('sha256').update('agindow-owner-v1:foreign').digest('hex');
    header.kid = header.kid.replace(ownerName, foreignOwner);
    const forged = [b64(header), parts[1], parts[2]].join('.');
    const namespace = await fixture.namespace();
    const foreign = fixturePort(namespace.get(namespace.idFromName(foreignOwner)));
    await foreign.setTime(fixture.now);
    assert.equal(
      (
        await guard(
          forged,
          signer.proof(forged, 'POST', 'https://guard.invalid/v1/guard', fixture.now),
          'ingress-b',
        )
      ).status,
      503,
    );
    assert.equal(await foreign.retained(), 0);
    assert.equal((await guard(access, proof, 'ingress-b')).status, 401);
    assert.equal(fixture.dispatches, 2);

    await (await owner()).resetVerifier();
    assert.equal((await guard(access, proof, 'ingress-b')).status, 503);
    assert.equal(fixture.dispatches, 2);
    assert.equal((await exchange('first')).status, 401);
    const refreshed = await exchange('after-verifier-reset');
    assert.equal(refreshed.status, 200);
    const freshAccess = /** @type {{access_token:string}} */ (await refreshed.json()).access_token;
    assert.notEqual(freshAccess.split('.')[0], access.split('.')[0]);
    await fixture.restart();
    await (await owner()).setTime(fixture.now);
    assert.equal((await exchange('first')).status, 401);
    assert.equal(
      (
        await guard(
          freshAccess,
          signer.proof(freshAccess, 'POST', 'https://guard.invalid/v1/guard', fixture.now),
          'ingress-a',
        )
      ).status,
      503,
    );
    const next = await exchange('after-owner-reset');
    assert.equal(next.status, 200);
    const nextAccess = /** @type {{access_token:string}} */ (await next.json()).access_token;
    await (await owner()).eraseLedger();
    const available = await guard(
      nextAccess,
      signer.proof(nextAccess, 'POST', 'https://guard.invalid/v1/guard', fixture.now),
      'ingress-b',
    );
    assert.equal(available.status, 200);
    assert.equal(await available.text(), ' {"owner":true}\n');
    assert.equal(fixture.dispatches, 3);
    assert.equal((await exchange('storage-failure')).status, 503);
    await fixture.restart();
    await (await owner()).setTime(fixture.now);
    assert.equal((await exchange('after-storage-loss')).status, 503);
    await fixture.setTime(fixture.now + 361);
    assert.equal((await exchange('new-after-quarantine')).status, 200);
    const recoveryIssued = await exchange('recovery', 'ingress-a', ['recovery']);
    assert.equal(recoveryIssued.status, 200);
    const recoveryToken = /** @type {{access_token:string}} */ (await recoveryIssued.json())
      .access_token;
    const held = await (
      await ingress()
    ).fetch('https://guard.invalid/v1/guard', {
      method: 'POST',
      headers: {
        authorization: `DPoP ${recoveryToken}`,
        dpop: signer.proof(recoveryToken, 'POST', 'https://guard.invalid/v1/guard', fixture.now),
      },
      body: JSON.stringify({
        tool: 'recovery',
        connection: 'c',
        operation: 'write',
        args: { value: 100 },
        mode: 'repair',
        idempotency_key: 'same',
      }),
    });
    assert.equal(held.status, 409);
    const location = held.headers.get('x-agindow-outcome');
    assert.ok(location);
    const outcomeUrl = new URL(location, 'https://guard.invalid').href;
    const feedback = /** @type {{audit:string}} */ (await held.json());
    const auditUrl = new URL(feedback.audit, 'https://guard.invalid').href;
    for (const [url, status] of /** @type {const} */ ([
      [outcomeUrl, 201],
      [auditUrl, 200],
    ])) {
      const getProof = signer.proof(recoveryToken, 'GET', url, fixture.now);
      /** @param {string} name */
      const retrieve = async (name) =>
        (await ingress(name)).fetch(url, {
          headers: { authorization: `DPoP ${recoveryToken}`, dpop: getProof },
        });
      const responses = await Promise.all([retrieve('ingress-a'), retrieve('ingress-b')]);
      assert.deepEqual(
        responses.map((response) => response.status).sort((a, b) => a - b),
        [status, 404],
      );
      for (const response of responses) assert.ok(!(await response.text()).includes(recoveryToken));
    }
    await (await owner()).fillLedger(fixture.now + 300);
    const retained = await (await owner()).retained();
    assert.equal((await exchange('capacity')).status, 429);
    assert.equal(await (await owner()).retained(), retained);
    await (await owner()).setTime(fixture.now - 100);
    assert.equal((await exchange('rollback')).status, 503);
  } finally {
    await fixture.close();
  }
});
