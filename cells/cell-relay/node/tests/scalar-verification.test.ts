import {expect, it} from 'vitest';
import {selectiveEngine, repairOptions} from '../src/selective.js';
import type {Engine, Verdict} from '../src/types.js';
import type {ScalarRule} from '../src/scalar.js';
const pass: Verdict = {halt:false,bypassed:false,class:null,findings:0,detectors:0,malformed:false};
const rule: ScalarRule = {recipe:'scalar',tool:'pay',side:'request',migration:'r',confidence:1,
  facts:{version:1,recipe:'string-map',tool:'pay',side:'request',contract:'v2',migration:'r',provenance:'review',expiresAt:4102444800000,pointer:'/value',mapping:[['old','new']]}};
function base(request: Verdict = pass): Engine { return {begin:()=>({request,finish:()=>pass,close(){}})}; }
it('independent verifier rejects guest mutations of source, context and direction',()=>{
  for(const body of ['{"value":"wrong","id":7}','{"value":"new","id":8}','{"value":null,"id":7}']) {
    const engine=selectiveEngine(base(),{rules:[rule],verify:()=>true},()=>JSON.stringify({matched:true,applied:true,halt:true,body}));
    expect(()=>engine.begin({value:'old',id:7},'repair','write',true,'pay')).toThrow('preservation');
  }
});
it('base policy, warming, malformed and prior statistical findings cannot be cleared',()=>{
  for(const request of [{...pass,halt:true},{...pass,bypassed:true},{...pass,malformed:true},{...pass,findings:1}]) {
    const engine=selectiveEngine(base(request),{rules:[rule],verify:()=>{throw Error('must not verify');}},()=>{throw Error('must not prepare');});
    expect(engine.begin({value:'old'},'repair','write',true,'pay').request).toBe(request);
  }
});
it('expiry recheck after verifier preserves semantic refusal and reads stay open',()=>{
  const expiring={...rule,facts:{...rule.facts,expiresAt:Date.now()+100_000}};
  const prepared=()=>JSON.stringify({matched:true,applied:true,halt:true,body:'{"value":"new"}'});
  const options=repairOptions({rules:[expiring],verify:()=>true},false);
  expect(options).toBeDefined();
  if (!options) throw Error('options');
  const now=Date.now;const expiry=expiring.facts.expiresAt;
  try {
    const engine=selectiveEngine(base(),{...options,verify:()=>{Date.now=()=>expiry;return true;}},prepared);
    expect(engine.begin({value:'old'},'repair','write',false,'pay').request).toMatchObject({halt:true,class:'semantic'});
  } finally {Date.now=now;}
});
it('semantic policy and confidence cannot be downgraded by preparation metadata',()=>{
  const prepared=()=>JSON.stringify({matched:true,applied:true,halt:false,body:'{"value":"new"}'});
  for(const [selected,verify] of [[rule,()=>false],[{...rule,confidence:.989},()=>true]] as const) {
    const engine=selectiveEngine(base(),{rules:[selected],verify},prepared);
    expect(engine.begin({value:'old'},'repair','write',false,'pay').request).toMatchObject({halt:true,class:'semantic'});
  }
});
