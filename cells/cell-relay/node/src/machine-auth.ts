import { createHash, generateKeyPairSync, randomUUID, sign } from 'node:crypto';
import { AgindowSetupError } from './types.js';
export const CLOCK_SKEW_SECONDS = 30;
export const MAX_TOKEN_TTL_SECONDS = 900;
const b64 = (value: string | Uint8Array): string => Buffer.from(value).toString('base64url');
const digest = (value: string): string => createHash('sha256').update(value).digest('base64url');
export interface ProofSigner {
  readonly thumbprint: string;
  proof(token: string, method: string, target: string, now: number): string;
}
export function canonicalTarget(target: string): string {
  const url = new URL(target);
  if (!['http:', 'https:'].includes(url.protocol) || url.username || url.password) throw new AgindowSetupError('invalid proof target');
  url.search = ''; url.hash = ''; return url.href;
}
/** Ephemeral process-owned Ed25519 key. The exported surface cannot serialize it. */
export function createProofSigner(): ProofSigner {
  const { privateKey, publicKey } = generateKeyPairSync('ed25519');
  const jwk = publicKey.export({ format: 'jwk' });
  const thumbprint = digest(JSON.stringify({ crv: 'Ed25519', kty: 'OKP', x: jwk.x }));
  return Object.freeze({ thumbprint, proof(token: string, method: string, target: string, now: number): string {
    const h = b64(JSON.stringify({ alg:'EdDSA',typ:'dpop+jwt',jwk:{ kty:'OKP',crv:'Ed25519',x:jwk.x } }));
    const c = b64(JSON.stringify({ jti:randomUUID(),htm:method,htu:canonicalTarget(target),iat:Math.floor(now),ath:digest(token) }));
    const signed = `${h}.${c}`; return `${signed}.${b64(sign(null,Buffer.from(signed),privateKey))}`;
  } });
}
/** Acquire must exchange with the supplied sender thumbprint. It runs only at
 * setup or background refresh, and must validate its exchange server's identity. */
export interface AccessCredential { readonly accessToken:string; readonly expiresAt:number }
export interface RemoteAuthOptions {
  readonly acquire:(thumbprint:string)=>Promise<AccessCredential>;
  readonly signer?:ProofSigner;
  readonly now?:()=>number;
  readonly schedule?:boolean;
  readonly allowLegacyFallback?:boolean;
  readonly legacyKey?:string;
  readonly onDiagnostic?:(reason:'keyless_ready'|'refresh_unavailable'|'legacy_unbound')=>void;
}
/** Returns undefined when unavailable. Never initiates acquisition from a call. */
export interface RemoteAuth {
  headers(method:string,target:string):Readonly<Record<string,string>>|undefined;
  refresh():Promise<boolean>;
  close():void;
}
export function validLegacyKey(value:string):boolean { return /^agw_(live|test)_[A-Za-z0-9_-]{32,256}$/.test(value); }
class Session implements RemoteAuth {
  #credential:AccessCredential|undefined;
  #legacy:string|undefined;
  #timer:ReturnType<typeof setTimeout>|undefined;
  #pending:Promise<boolean>|undefined;
  #closed=false;
  readonly #options:RemoteAuthOptions;
  readonly #signer:ProofSigner;
  readonly #now:()=>number;
  constructor(options:RemoteAuthOptions) {
    this.#options=options; this.#signer=options.signer??createProofSigner(); this.#now=options.now??(()=>Date.now()/1000);
  }
  #diagnostic(reason:'keyless_ready'|'refresh_unavailable'|'legacy_unbound'):void {
    try { this.#options.onDiagnostic?.(reason); } catch { /* diagnostic only */ }
  }
  #schedule(delay:number):void {
    if(this.#closed||this.#options.schedule===false)return;
    clearTimeout(this.#timer);
    this.#timer=setTimeout(()=>{ void this.refresh(); },Math.min(2_147_483_647,Math.max(1000,delay*1000)));
    this.#timer.unref();
  }
  async setup():Promise<void> {
    if(await this.refresh())return;
    const legacy=this.#options.legacyKey;
    if(this.#options.allowLegacyFallback===true&&legacy!==undefined&&validLegacyKey(legacy)) {
      this.#legacy=legacy; this.#diagnostic('legacy_unbound'); return;
    }
    this.close(); throw new AgindowSetupError('keyless identity unavailable');
  }
  refresh():Promise<boolean> {
    if(this.#closed)return Promise.resolve(false);
    if(this.#pending!==undefined)return this.#pending;
    this.#pending=this.#acquire().finally(()=>{this.#pending=undefined;}); return this.#pending;
  }
  async #acquire():Promise<boolean> {
    try {
      const c=await this.#options.acquire(this.#signer.thumbprint);
      const now=this.#now();
      if(!Number.isFinite(now)||!Number.isSafeInteger(c.expiresAt)||c.expiresAt<=now||c.expiresAt-now>MAX_TOKEN_TTL_SECONDS
        ||!c.accessToken||c.accessToken.length>16384||/[\s\u007f]/.test(c.accessToken))throw Error('invalid exchange result');
      if(this.#closed)return false;
      this.#credential=Object.freeze({accessToken:c.accessToken,expiresAt:c.expiresAt}); this.#legacy=undefined;
      this.#diagnostic('keyless_ready'); this.#schedule(c.expiresAt-now-60); return true;
    } catch { this.#diagnostic('refresh_unavailable'); this.#schedule(5); return false; }
  }
  headers(method:string,target:string):Readonly<Record<string,string>>|undefined {
    if(this.#closed)return undefined;
    const c=this.#credential; const now=this.#now();
    if(c!==undefined&&Number.isFinite(now)&&now<c.expiresAt) {
      try { return {authorization:`DPoP ${c.accessToken}`,dpop:this.#signer.proof(c.accessToken,method,target,now)}; } catch { return undefined; }
    }
    if(this.#legacy!==undefined)return {authorization:`Bearer ${this.#legacy}`};
    return undefined;
  }
  close():void { this.#closed=true; clearTimeout(this.#timer); this.#credential=undefined; this.#legacy=undefined; }
}
/** Setup is explicit and asynchronous. Tenant authority comes from exchange,
 * never from a local SDK tenant option. Do not expose returned headers to agents. */
export async function createRemoteAuth(options:RemoteAuthOptions):Promise<RemoteAuth> {
  if(options.legacyKey!==undefined&&!validLegacyKey(options.legacyKey))throw new AgindowSetupError('invalid legacy key');
  const session=new Session(options); await session.setup(); return session;
}
