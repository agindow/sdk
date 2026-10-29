import { afterEach, expect, it, vi } from 'vitest';
import { rmSync, writeFileSync, symlinkSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { sourceFixCommand } from '../cli/source-fixes/command.js';
import { migrationFixture, NOW, requestSource } from './support/source-migration.js';
const roots:string[]=[];afterEach(()=>{vi.restoreAllMocks();for(const root of roots.splice(0))rmSync(root,{recursive:true,force:true});});
it('exports from bounded explicit local files and retains the customer checkout',async()=>{
 const input=await migrationFixture(roots),{evidence,...selection}=input,root=input.checkout.root;
 const path=join(root,'export.json');writeFileSync(join(root,'evidence.json'),JSON.stringify(evidence));writeFileSync(path,JSON.stringify({...selection,evidenceReference:'evidence.json'}));
 const output=vi.spyOn(console,'log').mockImplementation(()=>undefined);
 expect(await sourceFixCommand([path,'--as-of',new Date(NOW).toISOString()])).toBe(0);
 expect(JSON.parse(String(output.mock.calls.at(-1)?.[0]))).toMatchObject({status:'generated',tests:{state:'unvalidated'},publicationAuthorized:false});
 expect(readFileSync(join(root,'api.ts'),'utf8')).toBe(requestSource);
 for(const ref of ['https://vendor.test/private','file:///tmp/evidence.json','missing.json','link.json']){
  if(ref==='link.json')symlinkSync(join(root,'evidence.json'),join(root,ref));
  writeFileSync(path,JSON.stringify({...selection,evidenceReference:ref}));
  expect(await sourceFixCommand([path,'--as-of',new Date(NOW).toISOString()])).toBe(1);
  expect(JSON.parse(String(output.mock.calls.at(-1)?.[0]))).toMatchObject({status:'refused'});
 }
 expect(await sourceFixCommand([path])).toBe(1);
});
