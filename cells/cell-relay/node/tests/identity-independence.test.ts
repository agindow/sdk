import { afterEach, expect, it, vi } from 'vitest';
import { createAgindow } from '../src/index.js';
afterEach(()=>{vi.unstubAllGlobals();});
it('executes local guarded calls with no login when dashboard and auth are unreachable',async()=>{
 const network = vi.fn(async()=>{throw new Error('Dashboard and auth unavailable');});
 vi.stubGlobal('fetch',network);
 const sdk = createAgindow(); await sdk.ready;
 const payload = {id:'original',nested:{unit:'usd'}};
 const upstream = vi.fn(async()=>payload);
 const guarded = sdk.guard(upstream,{operation:'read'});
 expect(await guarded()).toBe(payload);
 expect(upstream).toHaveBeenCalledTimes(1);
 expect(network).not.toHaveBeenCalled();
});
