import { test } from 'node:test';
import assert from 'node:assert/strict';
import { Miniflare } from 'miniflare';
import { createHash,generateKeyPairSync,sign } from 'node:crypto';
import { fileURLToPath } from 'node:url';
/** @param {unknown} v */
const b64=v=>{const json=typeof v==='string'?v:JSON.stringify(v);if(json===undefined)throw Error('fixture encoding');return Buffer.from(json).toString('base64url');};
/** @param {string} v */
const hash=v=>createHash('sha256').update(v).digest('base64url');
/** @param {Record<string,unknown>} header @param {Record<string,unknown>} claims @param {import('node:crypto').KeyObject} key */
const jwt=(header,claims,key)=>{const s=`${b64(header)}.${b64(claims)}`;return `${s}.${sign(null,Buffer.from(s),key).toString('base64url')}`;};
test('real Worker admits keyless sender, rejects stolen tokens and never dispatches rejected calls',async()=>{
  const issuer=generateKeyPairSync('ed25519');const sender=generateKeyPairSync('ed25519');const attacker=generateKeyPairSync('ed25519');
  const x=issuer.publicKey.export({format:'jwk'}).x;
  const jkt=hash(JSON.stringify({crv:'Ed25519',kty:'OKP',x:sender.publicKey.export({format:'jwk'}).x}));
  const platform=generateKeyPairSync('ed25519');
  const now=Math.floor(Date.now()/1000); let dispatches=0;
  const claims={iss:'https://issuer.test',sub:'workload',aud:'agindow:guard',tenant:'a',environment:'test',connection:'c',tools:['op'],iat:now,nbf:now,exp:now+120,jti:'access',cnf:{jkt},agw_act:[]};
  const token=jwt({alg:'EdDSA',typ:'at+jwt',kid:'one'},claims,issuer.privateKey);
  const runtime=new Miniflare({modules:true,modulesRules:[{type:'CompiledWasm',include:['**/*.wasm']}],scriptPath:fileURLToPath(new URL('../../crates/http-host/build/index.js',import.meta.url)),compatibilityDate:'2026-08-06',bindings:{
    GUARD_IDENTITY_EXCHANGE:JSON.stringify({issuer:'https://issuer.test',kid:'one',signing_seed:issuer.privateKey.export({format:'jwk'}).d,
      keys:[{issuer:'https://platform.test',kid:'platform',x:platform.publicKey.export({format:'jwk'}).x,until:now+300},{issuer:'https://issuer.test',kid:'one',x,until:now+300}],
      registrations:[{id:'workload',kid:'platform',audiences:['agindow:guard'],issuer:'https://platform.test',subject:'repo:acme/agent:main',audience:'agindow',tenant:'a',environment:'test',connection:'c',tools:['op'],current:true,private_key_jwt:false}]}),
    GUARD_MACHINE_AUTH:JSON.stringify({issuer:claims.iss,keys:[{issuer:claims.iss,kid:'one',x,until:now+300}],keyless_only:['a'],legacy_until:0}),
    GUARD_CONNECTIONS:JSON.stringify({'c/op':{tenant:'a',environment:'test',url:'https://vendor.invalid/action',method:'POST',operation:'read'}})},
    outboundService:async()=>{dispatches++;return new Response(' {"raw":1e400}\n');}});
  /** @param {{publicKey:import('node:crypto').KeyObject,privateKey:import('node:crypto').KeyObject}} key @param {string} id @param {string} [access] @param {string} [target] */
  const proof=(key,id,access=token,target='https://guard.invalid/v1/guard')=>jwt({alg:'EdDSA',typ:'dpop+jwt',jwk:{kty:'OKP',crv:'Ed25519',x:key.publicKey.export({format:'jwk'}).x}},
    {jti:id,htm:'POST',htu:target,iat:now,ath:hash(access)},key.privateKey);
  /** @param {string} [dpop] @param {string} [access] */
  const send=(dpop,access=token)=>runtime.dispatchFetch('https://guard.invalid/v1/guard',{method:'POST',headers:{authorization:`DPoP ${access}`,...(dpop?{dpop}:{})},body:JSON.stringify({tool:'op',connection:'c',operation:'read',args:{},mode:'observe'})});
  try {
    assert.equal((await send()).status,401);
    assert.equal((await send(proof(attacker,'stolen'))).status,401);assert.equal(dispatches,0);
    const legitimate=proof(sender,'legit');const response=await send(legitimate);
    assert.equal(response.status,200);assert.equal(await response.text(),' {"raw":1e400}\n');
    assert.equal((await send(legitimate)).status,401);assert.equal(dispatches,1);
    const cross=jwt({alg:'EdDSA',typ:'at+jwt',kid:'one'},{...claims,tenant:'b'},issuer.privateKey);
    assert.equal((await send(proof(sender,'cross'),cross)).status,403);assert.equal(dispatches,1);
    const assertion=jwt({alg:'EdDSA',typ:'JWT',kid:'platform'},{iss:'https://platform.test',sub:'repo:acme/agent:main',aud:'agindow',iat:now,nbf:now,exp:now+120},platform.privateKey);
    const base={grant_type:'urn:ietf:params:oauth:grant-type:token-exchange',subject_token_type:'urn:ietf:params:oauth:token-type:jwt',subject_token:assertion,registration:'workload',audience:'agindow:guard',tools:['op'],ttl:120,cnf:{jkt}};
    const issued=await runtime.dispatchFetch('https://guard.invalid/v1/token',{method:'POST',body:JSON.stringify(base)});
    assert.equal(issued.status,200);assert.equal(issued.headers.get('cache-control'),'no-store');
    const issuedBody=/** @type {{access_token:string}} */(await issued.json());const access=issuedBody.access_token;
    assert.equal((await send(proof(sender,'exchanged',access),access)).status,200);assert.equal(dispatches,2);
    const denied=await runtime.dispatchFetch('https://guard.invalid/v1/token',{method:'POST',body:JSON.stringify({...base,tools:['delete']})});
    assert.equal(denied.status,403);assert.ok(!(await denied.text()).includes(assertion));
    const delegateJkt=hash(JSON.stringify({crv:'Ed25519',kty:'OKP',x:attacker.publicKey.export({format:'jwk'}).x}));
    const delegated=await runtime.dispatchFetch('https://guard.invalid/v1/token',{method:'POST',headers:{dpop:proof(sender,'delegate',access,'https://guard.invalid/v1/token')},body:JSON.stringify({...base,subject_token_type:'urn:ietf:params:oauth:token-type:access_token',subject_token:access,connection:'c',subject:'child',ttl:60,cnf:{jkt:delegateJkt}})});
    assert.equal(delegated.status,200);const child=/** @type {{access_token:string}} */(await delegated.json());
    assert.equal((await send(proof(sender,'forwarded',child.access_token),child.access_token)).status,401);
    assert.equal((await send(proof(attacker,'child',child.access_token),child.access_token)).status,200);assert.equal(dispatches,3);
  }finally{await runtime.dispose();}
});
test('retained keyless outcomes and audits require fresh GET possession proofs',async()=>{
  const issuer=generateKeyPairSync('ed25519'),sender=generateKeyPairSync('ed25519');
  const now=Math.floor(Date.now()/1000),x=issuer.publicKey.export({format:'jwk'}).x;
  const jwk={kty:'OKP',crv:'Ed25519',x:sender.publicKey.export({format:'jwk'}).x};
  const jkt=hash(JSON.stringify({crv:jwk.crv,kty:jwk.kty,x:jwk.x}));
  const token=jwt({alg:'EdDSA',typ:'at+jwt',kid:'one'},{iss:'https://issuer.test',sub:'worker',aud:'agindow:guard',tenant:'a',environment:'test',connection:'c',tools:['op'],iat:now,nbf:now,exp:now+120,jti:'retained-access',cnf:{jkt},agw_act:[]},issuer.privateKey);
  /** @param {string} method @param {string} target @param {string} id */
  const proof=(method,target,id)=>jwt({alg:'EdDSA',typ:'dpop+jwt',jwk},{htm:method,htu:target,jti:id,iat:now,ath:hash(token)},sender.privateKey);
  const runtime=new Miniflare({modules:true,modulesRules:[{type:'CompiledWasm',include:['**/*.wasm']}],scriptPath:fileURLToPath(new URL('../../crates/http-host/build/index.js',import.meta.url)),compatibilityDate:'2026-08-06',bindings:{
    GUARD_MACHINE_AUTH:JSON.stringify({issuer:'https://issuer.test',keys:[{issuer:'https://issuer.test',kid:'one',x,until:now+300}],keyless_only:['a'],legacy_until:0}),
    GUARD_CONNECTIONS:JSON.stringify({'c/op':{tenant:'a',environment:'test',url:'https://vendor.invalid/action',method:'POST',operation:'write',selection:{tenant:'a',connection:'c',vendor:'v',operation:'op',segment:'s',fields:[{side:'response',path:'/value',baseline:Array.from({length:128},()=>'{"value":1}'),live:Array.from({length:128},()=>'{"value":100}'),enumeration:true}]}}})},
    outboundService:async()=>new Response(' {"value":100}\n',{status:201,headers:{'content-length':'15'}})});
  try{
    const response=await runtime.dispatchFetch('https://guard.invalid/v1/guard',{method:'POST',headers:{authorization:`DPoP ${token}`,dpop:proof('POST','https://guard.invalid/v1/guard','guard')},body:JSON.stringify({tool:'op',connection:'c',operation:'write',args:{value:100},mode:'repair',idempotency_key:'same'})});
    assert.equal(response.status,409);const location=response.headers.get('x-agindow-outcome');assert.ok(location);
    const url=new URL(location,'https://guard.invalid').href;
    assert.equal((await runtime.dispatchFetch(url,{headers:{authorization:`DPoP ${token}`}})).status,404);
    const dpop=proof('GET',url,'recovery');
    const recovered=await runtime.dispatchFetch(url,{headers:{authorization:`DPoP ${token}`,dpop}});
    assert.equal(recovered.status,201);assert.equal(await recovered.text(),' {"value":100}\n');
    assert.equal((await runtime.dispatchFetch(url,{headers:{authorization:`DPoP ${token}`,dpop}})).status,404);
    const feedback=/** @type {{audit:string}} */(await response.json());const auditUrl=new URL(feedback.audit,'https://guard.invalid').href;
    const audit=await runtime.dispatchFetch(auditUrl,{headers:{authorization:`DPoP ${token}`,dpop:proof('GET',auditUrl,'audit')}});
    assert.equal(audit.status,200);const text=await audit.text();assert.ok(text.includes('retained-access'));assert.ok(!text.includes(token));
  }finally{await runtime.dispose();}
});
