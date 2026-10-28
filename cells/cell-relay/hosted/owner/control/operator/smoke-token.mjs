#!/usr/bin/env node
/**
 * Private operator smoke: POST /v1/token with a workload PEM.
 * Prints status and a redacted access_token prefix only — never logs full tokens.
 *
 * usage: node smoke-token.mjs WORKLOAD_PRIVATE.pem
 */
import { createHash, createPrivateKey, generateKeyPairSync, randomUUID, sign } from 'node:crypto';
import { readFile } from 'node:fs/promises';

const pemPath = process.argv[2];
if (!pemPath || process.argv.length !== 3) {
  console.error('usage: node smoke-token.mjs WORKLOAD_PRIVATE.pem');
  process.exit(2);
}

const endpoint = 'https://guard.agindow.com/v1/token';
const registration = 'workload';
const kid = 'client-key';
const tools = ['op'];

const key = createPrivateKey(await readFile(pemPath));
if (key.asymmetricKeyType !== 'ed25519') throw Error('Ed25519 client key required');

/** @param {string | object} value */
const b64 = (value) => Buffer.from(typeof value === 'string' ? value : JSON.stringify(value)).toString('base64url');
/** @param {string} value */
const digest = (value) => createHash('sha256').update(value).digest('base64url');

const { privateKey: proofKey, publicKey: proofPub } = generateKeyPairSync('ed25519');
const proofJwk = proofPub.export({ format: 'jwk' });
const jkt = digest(JSON.stringify({ crv: 'Ed25519', kty: 'OKP', x: proofJwk.x }));

const now = Math.floor(Date.now() / 1000);
const header = b64({ alg: 'EdDSA', typ: 'JWT', kid });
const claims = b64({
  iss: registration,
  sub: registration,
  aud: endpoint,
  iat: now,
  nbf: now,
  exp: now + 120,
  jti: randomUUID(),
});
const signingInput = `${header}.${claims}`;
const assertion = `${signingInput}.${sign(null, Buffer.from(signingInput), key).toString('base64url')}`;

const body = {
  grant_type: 'urn:ietf:params:oauth:grant-type:token-exchange',
  subject_token_type: 'urn:ietf:params:oauth:token-type:jwt',
  subject_token: assertion,
  registration,
  audience: 'agindow:guard',
  tools,
  ttl: 300,
  cnf: { jkt },
};

const response = await fetch(endpoint, {
  method: 'POST',
  redirect: 'error',
  headers: { 'content-type': 'application/json' },
  body: JSON.stringify(body),
  signal: AbortSignal.timeout(10000),
});

const text = await response.text();
let parsed;
try {
  parsed = JSON.parse(text);
} catch {
  parsed = null;
}

console.log('status', response.status);
if (parsed && typeof parsed === 'object') {
  if (typeof parsed.error === 'string') console.log('error', parsed.error);
  if (typeof parsed.token_type === 'string') console.log('token_type', parsed.token_type);
  if (typeof parsed.expires_at === 'number') console.log('expires_at', parsed.expires_at, new Date(parsed.expires_at * 1000).toISOString());
  if (typeof parsed.access_token === 'string') {
    console.log('access_token_prefix', parsed.access_token.slice(0, 16) + '…');
    console.log('access_token_len', parsed.access_token.length);
  }
} else {
  console.log('body_prefix', text.slice(0, 200));
}

// Keep proof key alive in-process only so a follow-up guard smoke can import later if needed.
void proofKey;
process.exit(response.ok ? 0 : 1);
