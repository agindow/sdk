import {expect, it, vi} from 'vitest';
import {createAgindow} from '../src/index.js';
import {repairOptions, selectiveEngine} from '../src/selective.js';
import type {RenameRule, SelectiveRepairOptions} from '../src/selective.js';
import type {Engine, Verdict} from '../src/types.js';
const pass: Verdict = {halt:false,bypassed:false,class:null,findings:0,detectors:0,malformed:false};
const rule: RenameRule = {tool:'t',side:'request',from:'old',to:'next',migration:'m',confidence:1,
  facts:{version:1,recipe:'rename',tool:'t',side:'request',migration:'m',contract:'c',provenance:'p',expiresAt:4102444800000,
    parents:[''],old:{key:'old',type:'string'},new:{key:'next',type:'string'}}};
const options = (verify: SelectiveRepairOptions['verify'] = () => true): SelectiveRepairOptions => {
  const admitted = repairOptions({rules:[rule],verify},false);
  if (!admitted) throw Error('expected admission');
  return admitted;
};
const base = (verdict: Verdict = pass): Engine => ({begin:()=>({request:verdict,finish:()=>pass,close(){}})});
const prepared = (body: string): string => JSON.stringify({matched:true,applied:true,halt:false,body});
it('rejects candidate edits to untouched context before invoking independent verification', async()=>{
  let verified=0;
  const engine=selectiveEngine(base(),options(()=>{verified++;return true;}),()=>prepared('{"next":"Ada","id":2}'));
  const sdk=createAgindow({mode:'repair'},async()=>engine);await sdk.ready;
  const input={old:'Ada',id:1};expect(sdk.guard((x:object)=>x,{tool:'t'})(input)).toBe(input);
  expect(verified).toBe(0);expect(sdk.recent().filter(e=>e.type==='bypass')).toHaveLength(1);
});
it('cannot replace semantic findings or engine bypass with a reviewed candidate',()=>{
  for (const verdict of [{...pass,halt:true,class:'semantic' as const,findings:1}, {...pass,bypassed:true}]) {
    let preparedCount=0;
    const engine=selectiveEngine(base(verdict),options(),()=>{preparedCount++;return prepared('{"next":"Ada"}');});
    expect(engine.begin({old:'Ada'},'repair','write',true,'t').request).toBe(verdict);
    expect(preparedCount).toBe(0);
  }
});
it('verifier faults and asynchronous answers use once-per-call bypass with original identity',async()=>{
  for (const verify of [()=>{throw Error('fault');}, (()=>Promise.resolve(true)) as unknown as SelectiveRepairOptions['verify']]) {
    const engine=selectiveEngine(base(),options(verify),()=>prepared('{"next":"Ada"}'));
    const sdk=createAgindow({mode:'repair'},async()=>engine);await sdk.ready;
    const input={old:'Ada'};expect(sdk.guard((x:object)=>x,{tool:'t'})(input)).toBe(input);
    expect(sdk.recent().filter(e=>e.type==='bypass')).toHaveLength(1);
  }
});
it('guest failure and unavailable engine preserve original identity',async()=>{
  const engine=selectiveEngine({begin(){throw Error('guest failure');}},options(),()=>prepared('{"next":"Ada"}'));
  for (const load of [async()=>engine,async():Promise<Engine>=>{throw Error('not ready');}]) {
    const sdk=createAgindow({mode:'repair'},load);await sdk.ready;
    const input={old:'Ada'};expect(sdk.guard((x:object)=>x,{tool:'t'})(input)).toBe(input);
    expect(sdk.recent().filter(e=>e.type==='bypass')).toHaveLength(1);
  }
});
it('rejects guest audit, overlapping rules and facts rewritten by the verifier',()=>{
  expect(()=>repairOptions({rules:[rule],verify:()=>true},true)).toThrow(/audit/);
  expect(()=>repairOptions({rules:[rule,rule],verify:()=>true},false)).toThrow();
  const admitted=options(check=>{expect(Object.isFrozen('recipe' in check.rule ? undefined : check.rule.facts?.old)).toBe(true);return true;});
  expect(Object.isFrozen(admitted.rules[0]?.facts)).toBe(true);
});
it('requires independent pre/post facts even for an exact byte-preserving rename',()=>{
  let verified=0;
  const engine=selectiveEngine(base(),options(()=>{verified++;return true;}),()=>prepared('{"next":7}'));
  const verdict=engine.begin({old:7},'repair','write',true,'t').request;
  expect(verdict.repair).toBeUndefined();expect(verdict.findings).toBe(1);expect(verified).toBe(0);
});
it('rechecks fact expiry after the independent callback returns',()=>{
  const clock=vi.spyOn(Date,'now').mockReturnValue(1);
  try {
    const admitted=options(()=>{clock.mockReturnValue(4102444800001);return true;});
    const engine=selectiveEngine(base(),admitted,()=>prepared('{"next":"Ada"}'));
    expect(engine.begin({old:'Ada'},'repair','write',true,'t').request.repair).toBeUndefined();
  } finally { clock.mockRestore(); }
});
