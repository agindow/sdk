import { readFile, writeFile } from 'node:fs/promises';
import { createPrivateKey, createPublicKey, sign } from 'node:crypto';
import { pathToFileURL } from 'node:url';
import { build } from 'esbuild';
/** Offline only: read a public review request, sign with the operator key and write an exclusive private file.
 * @param {{request:string,key:string,out:string}} options
 */
export async function signReview(options) {
  const result=await build({entryPoints:[new URL('./protocol.ts',import.meta.url).pathname],bundle:true,write:false,platform:'node',format:'esm',logLevel:'silent'});
  const output=result.outputFiles?.[0]; if(!output) throw Error('compiler output unavailable');
  const protocol=/** @type {typeof import('./protocol.js')} */ (await import('data:text/javascript;base64,'+Buffer.from(output.text).toString('base64')));
  const request=await readFile(options.request,'utf8');
  if(request.length>300000) throw Error('request exceeds bound');
  const value=/** @type {Record<string,unknown>} */ (JSON.parse(request));
  const content=await protocol.reviewed(protocol.text(value['origin'],2048),protocol.text(value['profile']),value['rows']);
  const payload=/** @type {import('./protocol.js').Command} */ ({...value,...content});
  const key=createPrivateKey(await readFile(options.key));
  if(key.asymmetricKeyType!=='ed25519') throw Error('Ed25519 reviewer key required');
  const signed={payload,signature:sign(null,Buffer.from(protocol.encoded(payload)),key).toString('base64url')};
  const publicKey=createPublicKey(key).export({format:'jwk'});
  const keys=JSON.stringify([{kid:payload.kid,x:publicKey.x,until:payload.reviewUntil}]);
  // Verify closed fields/time/scope before writing; malformed requests never get an output artifact.
  await protocol.verified(JSON.stringify(signed),keys,Math.floor(Date.now()/1000),true);
  await writeFile(options.out,JSON.stringify(signed)+'\n',{flag:'wx',mode:0o600});
}
if(process.argv[1] && import.meta.url===pathToFileURL(process.argv[1]).href) {
  const [request,key,out,...extra]=process.argv.slice(2);
  if(!request||!key||!out||extra.length) throw Error('usage: node sign-review.mjs REQUEST.json REVIEWER_KEY.pem COMMAND.json');
  await signReview({request,key,out});
  console.log('Reviewed command written; no deployment or submission performed.');
}
