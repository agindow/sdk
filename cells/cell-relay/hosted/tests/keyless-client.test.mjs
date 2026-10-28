import { test, mock } from 'node:test';
import assert from 'node:assert/strict';
import { generateKeyPairSync } from 'node:crypto';
import { mkdtemp, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { transform } from 'esbuild';
import { Miniflare } from 'miniflare';
import { createProofSigner, guardRemote } from '../../node/dist/esm/index.js';
const { keylessNodeSetup } = /** @type {{keylessNodeSetup:string}} */ (
  await import(new URL('../../../../contracts/src/docs/keyless-example.ts', import.meta.url).href)
);

test('documented self-hosted client assertions exchange and call the real Worker without an API key', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'agindow-keyless-docs-'));
  const issuer = generateKeyPairSync('ed25519'),
    client = generateKeyPairSync('ed25519');
  const issuerJwk = issuer.privateKey.export({ format: 'jwk' }),
    clientJwk = client.publicKey.export({ format: 'jwk' });
  const now = Math.floor(Date.now() / 1000);
  let dispatches = 0,
    fallbacks = 0;
  const runtime = new Miniflare({
    modules: true,
    modulesRules: [{ type: 'CompiledWasm', include: ['**/*.wasm'] }],
    scriptPath: fileURLToPath(new URL('../../crates/http-host/build/index.js', import.meta.url)),
    compatibilityDate: '2026-08-06',
    bindings: {
      GUARD_IDENTITY_EXCHANGE: JSON.stringify({
        issuer: 'https://guard.invalid',
        kid: 'issuer-one',
        signing_seed: issuerJwk.d,
        keys: [{ issuer: 'agent-client', kid: 'client-one', x: clientJwk.x, until: now + 300 }],
        registrations: [
          {
            id: 'agent-client',
            kid: 'client-one',
            audiences: ['agindow:guard'],
            issuer: 'agent-client',
            subject: 'agent-client',
            audience: 'https://guard.invalid/v1/token',
            tenant: 'a',
            environment: 'test',
            connection: 'c',
            tools: ['op'],
            current: true,
            private_key_jwt: true,
          },
        ],
      }),
      GUARD_MACHINE_AUTH: JSON.stringify({
        issuer: 'https://guard.invalid',
        keys: [
          { issuer: 'https://guard.invalid', kid: 'issuer-one', x: issuerJwk.x, until: now + 300 },
        ],
        keyless_only: ['a'],
        legacy_until: 0,
      }),
      GUARD_CONNECTIONS: JSON.stringify({
        'c/op': {
          tenant: 'a',
          environment: 'test',
          url: 'https://vendor.invalid/read',
          method: 'POST',
          operation: 'read',
        },
      }),
    },
    outboundService: async () => {
      dispatches++;
      return new Response(' {"self_hosted":true}\n');
    },
  });
  const keyFile = join(directory, 'client.pem');
  await writeFile(keyFile, client.privateKey.export({ format: 'pem', type: 'pkcs8' }), {
    mode: 0o600,
  });
  const sdkUrl = new URL('../../node/dist/esm/index.js', import.meta.url).href;
  const compiled = await transform(
    keylessNodeSetup.replaceAll("'agindow'", JSON.stringify(sdkUrl)),
    { loader: 'ts', format: 'esm', target: 'es2022' },
  );
  const moduleFile = join(directory, 'setup.mjs');
  await writeFile(moduleFile, compiled.code);
  const { connectSelfHosted } =
    /** @type {{connectSelfHosted:(options:{exchangeEndpoint:string,registration:string,clientId:string,kid:string,audience:string,privateKeyFile:string,tools:string[]})=>Promise<import('../../node/dist/esm/index.js').RemoteAuth>}} */ (
      await import(pathToFileURL(moduleFile).href)
    );
  let assertionBody = '';
  const transport = mock.method(
    globalThis,
    'fetch',
    async (
      /** @type {Parameters<typeof fetch>[0]} */ input,
      /** @type {RequestInit|undefined} */ init,
    ) => {
      const url = String(input);
      if (url.endsWith('/v1/token')) assertionBody = String(init?.body);
      const request = new Request(input, init);
      return runtime.dispatchFetch(request.url, {
        method: request.method,
        headers: [...request.headers],
        ...(request.body === null ? {} : { body: await request.text() }),
      });
    },
  );
  /** @type {import('../../node/dist/esm/index.js').RemoteAuth|undefined} */
  let auth;
  const options = {
    exchangeEndpoint: 'https://guard.invalid/v1/token',
    registration: 'agent-client',
    clientId: 'agent-client',
    kid: 'client-one',
    audience: 'https://guard.invalid/v1/token',
    privateKeyFile: keyFile,
    tools: ['op'],
  };
  try {
    auth = await connectSelfHosted(options);
    const exchangeBody = assertionBody;
    const response = await guardRemote(
      () => {
        fallbacks++;
        return new Response('fallback');
      },
      { tool: 'op', connection: 'c', operation: 'read', args: {}, mode: 'observe' },
      { endpoint: 'https://guard.invalid/v1/guard', auth },
    );
    assert.equal(response.status, 200);
    assert.equal(await response.text(), ' {"self_hosted":true}\n');
    assert.equal(fallbacks, 0);
    assert.equal(dispatches, 1);
    const replay = await runtime.dispatchFetch(options.exchangeEndpoint, {
      method: 'POST',
      body: exchangeBody,
    });
    assert.equal(replay.status, 401);
    const headers = auth.headers('POST', 'https://guard.invalid/v1/guard');
    assert.ok(headers);
    const access = headers.authorization?.slice('DPoP '.length);
    assert.ok(access);
    const stolen = createProofSigner().proof(
      access,
      'POST',
      'https://guard.invalid/v1/guard',
      Date.now() / 1000,
    );
    const refused = await runtime.dispatchFetch('https://guard.invalid/v1/guard', {
      method: 'POST',
      headers: { authorization: `DPoP ${access}`, dpop: stolen },
      body: JSON.stringify({
        tool: 'op',
        connection: 'c',
        operation: 'read',
        args: {},
        mode: 'observe',
      }),
    });
    assert.equal(refused.status, 401);
    assert.equal(dispatches, 1);
    await assert.rejects(connectSelfHosted({ ...options, tools: ['delete'] }), {
      name: 'AgindowSetupError',
    });
    assert.equal(dispatches, 1);
    assert.equal(await auth.refresh(), true);
  } finally {
    auth?.close();
    transport.mock.restore();
    await runtime.dispose();
    await rm(directory, { recursive: true, force: true });
  }
});
