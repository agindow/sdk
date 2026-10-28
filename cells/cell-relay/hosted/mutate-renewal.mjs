import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
const cwd=fileURLToPath(new URL('../',import.meta.url));
for(const [mutation,file] of [
  ['digest','renewal-policy'],['revision','renewal-state'],['status','renewal-policy'],['expiry','renewal-policy'],
  ['cadence','renewal-policy'],['priority','renewal-policy'],['generation','renewal-security'],['hotpath','renewal-runtime'],['alarm','renewal-runtime'],
]) {
  const run=spawnSync(process.execPath,['--test',`hosted/tests/${file}.test.mjs`],{cwd,encoding:'utf8',env:{...process.env,AGINDOW_RENEWAL_MUTATION:mutation}});
  if(run.error||run.signal||run.status!==1||!run.stdout.includes('ERR_ASSERTION')||run.stdout.includes('mutation target changed')) throw Error(`renewal mutation ${mutation} did not fail behavior`);
  console.log(`renewal mutation rejected: ${mutation}`);
}
