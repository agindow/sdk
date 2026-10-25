import { describe, expect, it, vi } from 'vitest';
import fixture from '../parity/auth.json';
import { createRemoteAuth, createProofSigner } from '../src/machine-auth.js';
import { guardRemote } from '../src/remote.js';
import { createPublicKey, verify } from 'node:crypto';
const envelope = { tool:'read',connection:'c',operation:'read',args:{},mode:'observe' } as const;
describe('keyless SDK boundary', () => {
  it('acquires at setup, signs inside the process, and never acquires in a guard call', async () => {
    let now = fixture.now;
    const acquire = vi.fn(async () => ({ accessToken:'fixture-access-secret',expiresAt:fixture.expiresAt }));
    const auth = await createRemoteAuth({ acquire, now:() => now, schedule:false });
    expect(acquire).toHaveBeenCalledOnce();
    const headers = auth.headers('POST',fixture.target);
    expect(headers?.['authorization']).toBe('DPoP fixture-access-secret');
    const proof = headers?.['dpop'] ?? '';
    const [h,c,s] = proof.split('.');
    const header = JSON.parse(Buffer.from(h ?? '', 'base64url').toString()) as { jwk:JsonWebKey };
    const body = JSON.parse(Buffer.from(c ?? '', 'base64url').toString()) as { htu:string };
    expect(body.htu).toBe(fixture.canonical);
    expect(verify(null,Buffer.from(`${h}.${c}`),createPublicKey({key:header.jwk,format:'jwk'}),Buffer.from(s ?? '', 'base64url'))).toBe(true);
    const fetcher = vi.spyOn(globalThis,'fetch').mockResolvedValue(new Response('ok'));
    await guardRemote(() => 'fallback',envelope,{endpoint:fixture.target,auth});
    expect(acquire).toHaveBeenCalledOnce(); expect(fetcher).toHaveBeenCalledOnce();
    expect(JSON.stringify(auth)).not.toContain('fixture-access-secret');
    now=fixture.expiredAt;
    const events:unknown[]=[];
    expect(await guardRemote(() => 'fallback',envelope,{endpoint:fixture.target,auth,onEvent:e=>events.push(e)})).toBe('fallback');
    expect(events).toHaveLength(1); expect(fetcher).toHaveBeenCalledOnce();
    expect(JSON.stringify(events)).not.toContain('fixture-access-secret');
    fetcher.mockRestore(); auth.close();
  });
  it('retains a valid token on refresh failure, then bypasses; never silently falls back',async () => {
    let now=fixture.now; let fail=false;
    const auth=await createRemoteAuth({ now:()=>now,schedule:false,acquire:async()=>{ if(fail)throw Error('fixture-assertion-secret'); return {accessToken:'token',expiresAt:fixture.expiresAt}; } });
    fail=true; now=fixture.refreshAt; expect(await auth.refresh()).toBe(false);
    expect(auth.headers('POST',fixture.target)).toBeDefined();
    now=fixture.expiredAt; expect(auth.headers('POST',fixture.target)).toBeUndefined();
    await expect(guardRemote(()=>'',envelope,{endpoint:fixture.target,auth,apiKey:'agw_test_legacy'})).rejects.toThrow(/fallback/);
    auth.close();
  });
  it('contains acquisition errors at setup and labels explicit fallback',async()=>{
    const diagnostics:string[]=[];
    const options={acquire:async()=>{throw Error('fixture-assertion-secret');},schedule:false as const,onDiagnostic:(s:string)=>diagnostics.push(s)};
    await expect(createRemoteAuth(options)).rejects.toThrow('keyless identity unavailable');
    const auth=await createRemoteAuth({...options,allowLegacyFallback:true,legacyKey:'agw_test_0123456789abcdef0123456789abcdef'});
    expect(auth.headers('POST',fixture.target)?.['authorization']).toMatch(/^Bearer /);
    expect(diagnostics).toContain('legacy_unbound');
    expect(JSON.stringify(diagnostics)).not.toContain('fixture-assertion-secret'); auth.close();
  });
  it('keeps signer private and emits reproducible public thumbprints',()=>{
    const signer=createProofSigner(); expect(signer.thumbprint).toHaveLength(43);
    expect(JSON.stringify(signer)).not.toContain('PRIVATE');
  });
});
it('refuses malformed direct legacy keys before transport', async () => {
  await expect(guardRemote(()=>'',envelope,{endpoint:fixture.target,apiKey:'unprefixed-secret'})).rejects.toThrow('invalid legacy key');
});
it('automatically refreshes sixty seconds before expiry',async()=>{
  vi.useFakeTimers();let now=fixture.now;
  const acquire=vi.fn(async()=>({accessToken:'fixture-token',expiresAt:now+300}));
  let auth:Awaited<ReturnType<typeof createRemoteAuth>>|undefined;
  try{
    auth=await createRemoteAuth({acquire,now:()=>now});expect(acquire).toHaveBeenCalledOnce();
    now=fixture.refreshAt;await vi.advanceTimersByTimeAsync(240000);
    expect(acquire).toHaveBeenCalledTimes(2);expect(auth.headers('POST',fixture.target)).toBeDefined();
  }finally{auth?.close();vi.useRealTimers();}
});
