import {record,type Registration} from '../policy.js';
import {exact,text} from './protocol.js';
export function readCatalogBindings(raw:string,row:Registration):string{
 if(new TextEncoder().encode(raw).byteLength>262144||row.environment!=='test')throw Error('read_catalog');
 const catalog=record(JSON.parse(raw)),bindings:Record<string,{tenant:string;environment:'test';url:string;method:string;operation:'read'}>={};
 for(const tool of row.tools){
  const id=row.connection+'/'+tool,entry=record(catalog[id]);exact(entry,['url','method','operation']);
  const address=text(entry.url,2048),url=new URL(address);
  if(url.protocol!=='https:'||url.username||url.password||/[\s*]/u.test(address)||entry.operation!=='read'||!['GET','POST'].includes(String(entry.method)))throw Error('read_catalog');
  bindings[id]={tenant:row.tenant,environment:'test',url:address,method:String(entry.method),operation:'read'};
 }
 return JSON.stringify(bindings);
}
export function connectionsFor(env:{GUARD_CONNECTIONS:string;KEYLESS_MULTI_ORG?:string;KEYLESS_READ_CATALOG?:string},row:Registration):string{
 if(env.KEYLESS_MULTI_ORG==='true'){
  if(!env.KEYLESS_READ_CATALOG)throw Error('read_catalog');return readCatalogBindings(env.KEYLESS_READ_CATALOG,row);
 }
 return env.GUARD_CONNECTIONS;
}
