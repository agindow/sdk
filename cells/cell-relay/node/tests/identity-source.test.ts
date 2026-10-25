import {expect,it,vi} from 'vitest';
import {workloadIdentity} from '../src/identity-source.js';
import {createRemoteAuth} from '../src/machine-auth.js';
it('exchanges only registered scope and sender binding; discards failure response bytes',async()=>{
  const fetcher=vi.spyOn(globalThis,'fetch').mockResolvedValue(new Response(JSON.stringify({access_token:'fixture-token',expires_at:1300,token_type:'DPoP'})));
  try {
    const acquire=workloadIdentity({exchangeEndpoint:'https://agindow.test/v1/token',registration:'r',tools:['read'],assertion:async()=> 'fixture-platform-assertion'});
    const auth=await createRemoteAuth({acquire,now:()=>1000,schedule:false});
    expect(fetcher).toHaveBeenCalledOnce();
    const call=fetcher.mock.calls[0];
    const body=JSON.parse(String(call?.[1]?.body)) as Record<string,unknown>;
    expect(body['registration']).toBe('r');expect(body['cnf']).toBeDefined();expect(body['tenant']).toBeUndefined();
    fetcher.mockResolvedValue(new Response('fixture-platform-assertion',{status:503}));
    expect(await auth.refresh()).toBe(false);
    expect(JSON.stringify(auth)).not.toContain('fixture-platform-assertion');auth.close();
  }finally{fetcher.mockRestore();}
});
it('rejects insecure exchange targets at setup',()=>{
  expect(()=>workloadIdentity({exchangeEndpoint:'http://agindow.test/v1/token',registration:'r',tools:['read']})).toThrow('invalid identity exchange endpoint');
});
