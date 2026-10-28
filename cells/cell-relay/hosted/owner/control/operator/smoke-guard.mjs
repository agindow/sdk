#!/usr/bin/env node
/**
 * Private operator smoke: /v1/token then DPoP POST /v1/guard (read/observe).
 * Redacts access tokens; prints status and a short upstream body prefix.
 *
 * usage: node smoke-guard.mjs WORKLOAD_PRIVATE.pem
 */
import { createHash, createPrivateKey, generateKeyPairSync, randomUUID, sign } from 'node:crypto';
import { readFile } from 'node:fs/promises';

const pemPath = process.argv[2];
if (!pemPath || process.argv.length !== 3) {
  console.error('usage: node smoke-guard.mjs WORKLOAD_PRIVATE.pem');
  process.exit(2);
}

const tokenUrl = 'https://guard.agindow.com/v1/token';
const guardUrl = 'https://guard.agindow.com/v1/guard';
const registration = 'workload';
const kid = 'client-key';
const tools = ['op'];

const key = createPrivateKey(await readFile(pemPath));
if (key.asymmetricKeyType !== 'ed25519') throw Error('Ed25519 client key required');

/** @param {object} value */
const b64json = (value) => Buffer.from(JSON.stringify(value)).toString('base64url');
/** @param {Uint8Array} value */
const b64 = (value) => Buffer.from(value).toString('base64url');
/** @param {string} value */
const digest = (value) => createHash('sha256').update(value).digest('base64url');

const { privateKey: proofKey, publicKey: proofPub } = generateKeyPairSync('ed25519');
const proofJwk = proofPub.export({ format: 'jwk' });
const jkt = digest(JSON.stringify({ crv: 'Ed25519', kty: 'OKP', x: proofJwk.x }));

const now = Math.floor(Date.now() / 1000);
const assertHeader = b64json({ alg: 'EdDSA', typ: 'JWT', kid });
const assertClaims = b64json({
  iss: registration,
  sub: registration,
  aud: tokenUrl,
  iat: now,
  nbf: now,
  exp: now + 120,
  jti: randomUUID(),
});
const assertInput = `${assertHeader}.${assertClaims}`;
const assertion = `${assertInput}.${sign(null, Buffer.from(assertInput), key).toString('base64url')}`;

const tokenResponse = await fetch(tokenUrl, {
  method: 'POST',
  redirect: 'error',
  headers: { 'content-type': 'application/json' },
  body: JSON.stringify({
    grant_type: 'urn:ietf:params:oauth:grant-type:token-exchange',
    subject_token_type: 'urn:ietf:params:oauth:token-type:jwt',
    subject_token: assertion,
    registration,
    audience: 'agindow:guard',
    tools,
    ttl: 300,
    cnf: { jkt },
  }),
  signal: AbortSignal.timeout(10000),
});
const tokenText = await tokenResponse.text();
let tokenBody;
try {
  tokenBody = JSON.parse(tokenText);
} catch {
  tokenBody = null;
}
console.log('token_status', tokenResponse.status);
if (!tokenResponse.ok || !tokenBody || typeof tokenBody.access_token !== 'string') {
  if (tokenBody && typeof tokenBody.error === 'string') console.log('token_error', tokenBody.error);
  else console.log('token_body_prefix', tokenText.slice(0, 200));
  process.exit(1);
}
console.log('token_type', tokenBody.token_type);
console.log('access_token_prefix', tokenBody.access_token.slice(0, 16) + '…');

const access = tokenBody.access_token;
const dpopHeader = b64json({
  alg: 'EdDSA',
  typ: 'dpop+jwt',
  jwk: { kty: 'OKP', crv: 'Ed25519', x: proofJwk.x },
});
const dpopClaims = b64json({
  jti: randomUUID(),
  htm: 'POST',
  htu: guardUrl,
  iat: Math.floor(Date.now() / 1000),
  ath: digest(access),
});
const dpopInput = `${dpopHeader}.${dpopClaims}`;
const dpop = `${dpopInput}.${b64(sign(null, Buffer.from(dpopInput), proofKey))}`;

const guardResponse = await fetch(guardUrl, {
  method: 'POST',
  redirect: 'error',
  headers: {
    authorization: `DPoP ${access}`,
    dpop,
    'content-type': 'application/json',
  },
  body: JSON.stringify({
    tool: 'op',
    connection: 'c',
    operation: 'read',
    args: {},
    mode: 'observe',
  }),
  signal: AbortSignal.timeout(20000),
});
const guardText = await guardResponse.text();
console.log('guard_status', guardResponse.status);
console.log('guard_content_type', guardResponse.headers.get('content-type'));
for (const name of ['x-agindow-latency-ms', 'x-agindow-latency-clock', 'x-agindow-outcome']) {
  const value = guardResponse.headers.get(name);
  if (value) console.log(name, value);
}
console.log('guard_body_prefix', guardText.slice(0, 240).replace(/\s+/g, ' '));
process.exit(guardResponse.ok ? 0 : 1);
