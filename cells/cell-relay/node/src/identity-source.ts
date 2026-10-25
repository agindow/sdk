import { open } from 'node:fs/promises';
import { AgindowSetupError } from './types.js';
import type { AccessCredential } from './machine-auth.js';
export interface WorkloadIdentityOptions {
  readonly exchangeEndpoint:string;
  readonly registration:string;
  readonly tools:readonly string[];
  readonly audience?:string;
  readonly ttl?:number;
  readonly tokenFile?:string;
  readonly assertion?:()=>Promise<string>;
}
/** Build a setup/background acquisition callback. The issuer registration fixes
 * tenant authority; neither file nor request can choose a tenant/environment.
 * Supply tokenFile explicitly or AGINDOW_IDENTITY_TOKEN_FILE, never AGINDOW_KEY. */
export function workloadIdentity(options:WorkloadIdentityOptions):(thumbprint:string)=>Promise<AccessCredential> {
  let endpoint:URL;
  try { endpoint=new URL(options.exchangeEndpoint); } catch { throw new AgindowSetupError('invalid identity exchange endpoint'); }
  if(endpoint.protocol!=='https:'||endpoint.username||endpoint.password||endpoint.hash)throw new AgindowSetupError('invalid identity exchange endpoint');
  const file=options.tokenFile??process.env['AGINDOW_IDENTITY_TOKEN_FILE'];
  return async thumbprint=>{
    let assertion:string;
    if(options.assertion!==undefined)assertion=await options.assertion();
    else {
      if(file===undefined)throw new AgindowSetupError('workload identity unavailable');
      const handle=await open(file,'r');
      try {
        const buffer=Buffer.alloc(16385);const {bytesRead}=await handle.read(buffer,0,buffer.length,0);
        if(bytesRead>16384)throw Error('identity size');
        assertion=buffer.subarray(0,bytesRead).toString('utf8').trim();
      }finally{await handle.close();}
    }
    if(!assertion||assertion.length>16384||/[\s\u007f]/.test(assertion))throw Error('invalid identity assertion');
    const response=await fetch(endpoint,{method:'POST',redirect:'error',signal:AbortSignal.timeout(5000),headers:{'content-type':'application/json'},
      body:JSON.stringify({grant_type:'urn:ietf:params:oauth:grant-type:token-exchange',subject_token_type:'urn:ietf:params:oauth:token-type:jwt',
        subject_token:assertion,registration:options.registration,audience:options.audience??'agindow:guard',tools:options.tools,ttl:options.ttl??300,cnf:{jkt:thumbprint}})});
    if(!response.ok){void response.body?.cancel().catch(()=>{});throw Error('identity exchange unavailable');}
    const reader=response.body?.getReader();if(reader===undefined)throw Error('invalid exchange result');
    const chunks:Uint8Array[]=[];let length=0;
    try{for(;;){const c=await reader.read();if(c.done)break;length+=c.value.length;if(length>20000)throw Error('exchange size');chunks.push(c.value);}}
    finally{void reader.cancel().catch(()=>{});reader.releaseLock();}
    const value:unknown=JSON.parse(Buffer.concat(chunks,length).toString());
    if(value===null||typeof value!=='object'||Array.isArray(value))throw Error('invalid exchange result');
    const wire=value as Record<string,unknown>;
    if(typeof wire['access_token']!=='string'||typeof wire['expires_at']!=='number'||wire['token_type']!=='DPoP')throw Error('invalid exchange result');
    return {accessToken:wire['access_token'],expiresAt:wire['expires_at']};
  };
}
