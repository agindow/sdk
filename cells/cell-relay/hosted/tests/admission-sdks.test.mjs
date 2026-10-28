import {test,mock} from 'node:test';
import assert from 'node:assert/strict';
import {mkdtemp,writeFile,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join,delimiter} from 'node:path';
import {pathToFileURL,fileURLToPath} from 'node:url';
import {spawn} from 'node:child_process';
import {createServer} from 'node:http';
import {transform} from 'esbuild';
import {ownerHarness} from './support/owner-harness.mjs';
import {guardRemote} from '../../node/dist/esm/index.js';
const {keylessNodeSetup}=/** @type {{keylessNodeSetup:string}} */(await import(new URL('../../../../contracts/src/docs/keyless-example.ts',import.meta.url).href));
const {keylessPythonSetup}=/** @type {{keylessPythonSetup:string}} */(await import(new URL('../../../../contracts/src/docs/keyless-python-example.ts',import.meta.url).href));
const root=fileURLToPath(new URL('../../',import.meta.url));
test('published Node and Python helpers use a self-serve handoff for real sender-bound reads',async()=>{
 const f=await ownerHarness({auto:true,multi:true}),dir=await mkdtemp(join(tmpdir(),'agindow-admission-sdk-'));
 /** @type {ReturnType<typeof createServer>|undefined} */let bridge;
 /** @type {import('../../node/dist/esm/index.js').RemoteAuth|undefined} */let auth;
 const resets=[];
 try{
  const s=await f.admission();
  const identity=()=>JSON.stringify({actor:'owner',tenant:'a',environment:'test',role:'owner',authorizedAt:f.now,authenticatedAt:f.now});
  const created=JSON.parse(await s.mutate(identity(),'sdk-create',JSON.stringify({operation:'create',connection:'c',jwk:{kty:'OKP',crv:'Ed25519',kid:'client-key',x:f.client.publicKey.export({format:'jwk'}).x}})));
  const id=created.workloads[0].id;
  await (await f.control()).deliver();await f.setTime(f.now+361);await s.setTime(f.now);
  const w=JSON.parse(await s.list(identity())).workloads[0];assert.equal(w.status,'ready');
  const keyFile=join(dir,'workload-private.pem');await writeFile(keyFile,f.client.privateKey.export({type:'pkcs8',format:'pem'}),{mode:0o600});
  const sdk=new URL('../../node/dist/esm/index.js',import.meta.url).href;
  const compiled=await transform(keylessNodeSetup.replaceAll("'agindow'",JSON.stringify(sdk)),{loader:'ts',format:'esm',target:'es2022'});
  const helper=join(dir,'helper.mjs');await writeFile(helper,compiled.code);
  const {connectSelfHosted}=/** @type {{connectSelfHosted:(options:{exchangeEndpoint:string;registration:string;clientId:string;kid:string;audience:string;privateKeyFile:string;tools:string[]})=>Promise<import('../../node/dist/esm/index.js').RemoteAuth>}} */(await import(pathToFileURL(helper).href));
  resets.push(mock.method(Date,'now',()=>f.now*1000));
  resets.push(mock.method(globalThis,'fetch',async(/** @type {Parameters<typeof fetch>[0]} */ input,/** @type {RequestInit|undefined} */ init)=>{
   const request=new Request(input,init);return (await f.ingress()).fetch(request.url,{method:request.method,headers:[...request.headers],...(request.body?{body:await request.text()}:{})});
  }));
  auth=await connectSelfHosted({exchangeEndpoint:w.exchangeUrl,registration:id,clientId:id,kid:w.kid,audience:w.audience,privateKeyFile:keyFile,tools:w.tools});
  let bypass=0;
  const response=await guardRemote(async()=>{bypass++;return new Response('fallback');},{tool:'op',connection:'c',operation:'read',args:{},mode:'observe'},{endpoint:w.guardUrl,auth});
  assert.equal(response.status,200);assert.equal(await response.text(),' {"owner":true}\n');assert.equal(bypass,0);auth.close();auth=undefined;
  // Python's HTTPS transport is redirected only inside this fixture to the real local owner.
  bridge=createServer(async(req,res)=>{
   try{const chunks=[];for await(const chunk of req)chunks.push(chunk);const response=await (await f.ingress()).fetch('https://guard.invalid'+req.url,{method:req.method??'POST',headers:Object.entries(req.headers).flatMap(([k,v])=>typeof v==='string'?[[k,v]]:[]),body:Buffer.concat(chunks)});
    res.writeHead(response.status,Object.fromEntries(response.headers));res.end(Buffer.from(await response.arrayBuffer()));
   }catch{res.writeHead(503);res.end();}
  });
  const listener=bridge;await new Promise(resolve=>{listener.listen(0,'127.0.0.1',()=>resolve(undefined));});const address=listener.address();if(!address||typeof address==='string')throw Error('bridge');
  const script=`import time\ntime.time = lambda: ${f.now}\n`+keylessPythonSetup+`\nimport urllib.request as request\nfrom http.client import HTTPConnection\nimport agindow.identity_source as identity_source\nimport agindow.remote as remote\nfrom agindow import RemoteEnvelope, RemoteOptions, guard_remote\nclass Bridge:\n    def open(self, req, timeout):\n        target = request.Request("http://127.0.0.1:${address.port}" + req.full_url.replace("https://guard.invalid", ""), data=req.data, headers=dict(req.header_items()), method=req.method)\n        return request.urlopen(target, timeout=timeout)\nidentity_source.build_opener = lambda *args: Bridge()\nremote.HTTPSConnection = lambda *args, **kwargs: HTTPConnection("127.0.0.1", ${address.port}, **kwargs)\nauth = connect_self_hosted(exchange_endpoint=${JSON.stringify(w.exchangeUrl)}, registration=${JSON.stringify(id)}, client_id=${JSON.stringify(id)}, kid=${JSON.stringify(w.kid)}, audience=${JSON.stringify(w.audience)}, private_key_file=${JSON.stringify(keyFile)}, tools=("op",))\ndef original():\n    raise AssertionError("unexpected bypass")\ntry:\n    response = guard_remote(original, RemoteEnvelope(tool="op", connection="c", operation="read", args={}, mode="observe"), RemoteOptions(endpoint=${JSON.stringify(w.guardUrl)}, auth=auth))\n    assert response.status == 200\n    assert response.read() == b' {"owner":true}\\n'\n    response.close()\nfinally:\n    auth.close()\nprint("python-read-ok")\n`;
  const file=join(dir,'helper.py');await writeFile(file,script);
  /** @type {Promise<{code:number|null;out:string;err:string}>} */
  const completion=new Promise((resolve,reject)=>{
   const child=spawn(join(root,'python/.venv/bin/python'),[file],{env:{...process.env,PYTHONPATH:[join(root,'python/src'),process.env.PYTHONPATH??''].join(delimiter)},stdio:['ignore','pipe','pipe']});
   let out='',err='';child.stdout.on('data',chunk=>out+=chunk);child.stderr.on('data',chunk=>err+=chunk);child.on('error',reject);child.on('close',code=>resolve({code,out,err}));
  });
  const result=await completion;assert.equal(result.code,0,result.err);assert.equal(result.out.trim(),'python-read-ok');assert.equal(f.dispatches,2);
 }finally{
  auth?.close();for(const reset of resets)reset.mock.restore();
  if(bridge){const listener=bridge;await new Promise(resolve=>{listener.close(()=>resolve(undefined));});}await f.close();await rm(dir,{recursive:true,force:true});
 }
});
