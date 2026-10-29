import { afterEach, expect, it, vi } from 'vitest';
import { rmSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { exportSourceFix } from '../cli/source-fixes/export.js';
import { migrationFixture, NOW, requestSource } from './support/source-migration.js';
const roots:string[]=[];afterEach(()=>{for(const root of roots.splice(0))rmSync(root,{recursive:true,force:true});});
it('exports minimal patch, citation and honest tests with an injected local evidence source',async()=>{
 const input=await migrationFixture(roots);const read=vi.fn(async()=>input.evidence);
 const {evidence,...selection}=input;
 const result=await exportSourceFix({...selection,evidenceReference:'local-evidence'},NOW,{read});
 expect(read).toHaveBeenCalledExactlyOnceWith('local-evidence');
 expect(result).toMatchObject({status:'generated',patch:expect.stringContaining('payment_method'),citations:[{reference:'declaration-1'}],tests:{state:'unvalidated'},publicationAuthorized:false});
 expect(readFileSync(join(input.checkout.root,'api.ts'),'utf8')).toBe(requestSource);expect(evidence.digest).toBe(input.grant.evidenceDigest);
});
it('contains local evidence failures and refuses substituted evidence without echoing source',async()=>{
 const input=await migrationFixture(roots);const {evidence,...selection}=input;
 for(const read of [async()=>{throw Error('secret');},async()=>({...evidence,digest:'a'.repeat(64)})]){
  const result=await exportSourceFix({...selection,evidenceReference:'local'},NOW,{read});expect(result.status).toBe('refused');expect(JSON.stringify(result)).not.toContain('secret');
 }
});
