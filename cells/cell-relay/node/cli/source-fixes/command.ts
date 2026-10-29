import { closeSync, constants, fstatSync, lstatSync, openSync, readSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { exportSourceFix } from './export.js';
import { refused } from '../migration/types.js';
/** O_NOFOLLOW and bounded reads; no writes, code execution, remote URL or publisher fallback. */
function localJson(path:string,limit:number):unknown {
 if(!lstatSync(path).isFile())throw Error('Not a local file');
 const fd=openSync(path,constants.O_RDONLY|constants.O_NOFOLLOW);
 try{
  const stat=fstatSync(fd);if(!stat.isFile()||stat.size>limit)throw Error('File limit');
  const bytes=Buffer.alloc(limit+1),size=readSync(fd,bytes,0,bytes.length,0);
  if(size>limit)throw Error('File limit');return JSON.parse(bytes.subarray(0,size).toString('utf8')) as unknown;
 }finally{closeSync(fd);}
}
export async function sourceFixCommand(args:readonly string[]):Promise<number> {
 let result:Awaited<ReturnType<typeof exportSourceFix>>=refused('invalid_selection');
 const [path,flag,asOf]=args;
 if(args.length===3&&path&&flag==='--as-of'&&asOf&&/^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d(?:\.\d{3})?Z$/u.test(asOf)&&Number.isFinite(Date.parse(asOf)))try{
  const input=localJson(path,65536);
  result=await exportSourceFix(input,Date.parse(asOf),{read:async reference=>{
   if(/^[a-z][a-z0-9+.-]*:/iu.test(reference))throw Error('Local reference required');
   return localJson(resolve(dirname(path),reference),32768);
  }});
 }catch{/* Fixed typed refusal; private paths and provider narratives never enter output. */}
 console.log(JSON.stringify(result));return result.status==='refused'?1:0;
}
