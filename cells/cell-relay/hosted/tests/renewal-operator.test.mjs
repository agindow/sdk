import { test } from 'node:test';
import assert from 'node:assert/strict';
import { generateKeyPairSync } from 'node:crypto';
import { mkdtemp,writeFile,readFile,rm,stat } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { signReview } from '../owner/control/sign-review.mjs';
test('offline operator signing never submits or copies private-key bytes into the artifact',async()=>{
  const directory=await mkdtemp(join(tmpdir(),'agindow-review-'));
  try {
    const pair=generateKeyPairSync('ed25519'), now=Math.floor(Date.now()/1000);
    const key=join(directory,'reviewer.pem'),request=join(directory,'request.json'),out=join(directory,'command.json');
    const pem=pair.privateKey.export({format:'pem',type:'pkcs8'});
    await writeFile(key,pem,{mode:0o600});
    const content={version:1,origin:'https://guard.invalid',profile:'primary',generation:'generation',previousGeneration:null,expectedRevision:0,id:'command',kid:'reviewer',iat:now,exp:now+120,action:'admit',reviewUntil:now+86400,
      rows:[{id:'client',kid:'client-key',x:pair.publicKey.export({format:'jwk'}).x,audience:'https://guard.invalid/v1/token',tenant:'t',environment:'test',connection:'c',tools:['read']}]};
    await writeFile(request,JSON.stringify(content));
    await signReview({request,key,out});
    const command=await readFile(out,'utf8');
    assert.doesNotMatch(command,/PRIVATE KEY|signing_seed|delivery_token/u);
    assert.ok(!command.includes(String(pem)));
    assert.equal((await stat(out)).mode&0o777,0o600);
    await assert.rejects(signReview({request,key,out}));
    await writeFile(request,JSON.stringify({...content,extra:'unknown'}));
    await assert.rejects(signReview({request,key,out:join(directory,'bad.json')}));
  }finally{await rm(directory,{recursive:true,force:true});}
});
