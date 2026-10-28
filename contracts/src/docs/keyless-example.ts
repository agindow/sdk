/** Displayed setup helper; the hosted suite executes these exact bytes against the built SDK/Worker. */
export const keylessNodeSetup = `import { readFile } from 'node:fs/promises';
import { createPrivateKey, randomUUID, sign } from 'node:crypto';
import { createRemoteAuth, workloadIdentity } from 'agindow';

export async function connectSelfHosted(options: {
  exchangeEndpoint: string; registration: string;
  clientId: string; kid: string; audience: string;
  privateKeyFile: string; tools: string[];
}) {
  // This is the workload's key, held outside model context and source control.
  const key = createPrivateKey(await readFile(options.privateKeyFile));
  if (key.asymmetricKeyType !== 'ed25519') throw Error('Ed25519 client key required');
  const encode = (value: object) => Buffer.from(JSON.stringify(value)).toString('base64url');
  return createRemoteAuth({
    acquire: workloadIdentity({
      exchangeEndpoint: options.exchangeEndpoint,
      registration: options.registration, tools: options.tools,
      assertion: async () => {
        const now = Math.floor(Date.now() / 1000);
        const header = encode({ alg: 'EdDSA', typ: 'JWT', kid: options.kid });
        const claims = encode({ iss: options.clientId, sub: options.clientId,
          aud: options.audience, iat: now, nbf: now, exp: now + 120, jti: randomUUID() });
        const input = header + '.' + claims;
        return input + '.' + sign(null, Buffer.from(input), key).toString('base64url');
      },
    }),
  });
}`;
