import {test} from 'node:test';
import assert from 'node:assert/strict';
import {createAgindow, AgindowSetupError} from '../dist/esm/index.js';
/** @type {import('../src/scalar.js').ScalarScope} */
const scope={version:1,tool:'pay',side:'request',contract:'v2',migration:'review-1',provenance:'owner',expiresAt:4102444800000,pointer:'/value'};
/** @type {Extract<import('../src/scalar.js').ScalarFacts,{recipe:'string-map'}>} */
const mapping={...scope,recipe:'string-map',mapping:[['old','new']]};
/** @type {Extract<import('../src/scalar.js').ScalarFacts,{recipe:'scale'}>} */
const scaling={...scope,recipe:'scale',sourceUnit:'cents',targetUnit:'dollars',numerator:'1',denominator:'100',inputMin:'-999999999999999999',inputMax:'999999999999999999',outputMin:'-999999999999999999',outputMax:'999999999999999999'};
/** @param {import('../src/scalar.js').ScalarFacts} facts @returns {import('../src/scalar.js').ScalarRule} */
const rule=facts=>({recipe:'scalar',tool:facts.tool,side:facts.side,migration:facts.migration,confidence:1,facts});
/** @param {import('../src/scalar.js').ScalarFacts} [facts] @param {import('../src/types.js').Mode} [mode] @param {import('../src/selective.js').SelectiveRepairOptions['verify']} [verify] @param {Partial<import('../src/types.js').Options>} [extra] */
async function sdk(facts=mapping,mode='repair',verify=()=>true,extra={}) {
 const instance=createAgindow({mode,...extra,selectiveRepair:{rules:[rule(facts)],verify}});await instance.ready;return instance;
}
test('scalar string mapping preserves raw representations and untouched tokens',async()=>{
 const instance=await sdk(); const before=' {"value":"old", "id":9007199254740993,"huge":1e400} ';
 for(const input of [before,Buffer.from(before),new TextEncoder().encode(before)]) {
  let calls=0;const after=instance.guard((/** @type {string|Buffer|Uint8Array} */ x)=>{calls++;return x;},{tool:'pay',operation:'write',inputShape:'raw'})(input);
  assert.equal(typeof after,typeof input);assert.equal(after.constructor,input.constructor);
  assert.equal(typeof after==='string'?after:new TextDecoder().decode(after),before.replace('"old"','"new"'));assert.equal(calls,1);
 }
});
test('exact numeric scale above 2^53 and reverse epoch conversions',async()=>{
 /** @type {[import('../src/scalar.js').ScalarFacts,string,string][]} */
 const cases=[[scaling,'9007199254740993','90071992547409.93'],[scaling,'-125','-1.25'],[scaling,'0','0'],
  [{...scaling,sourceUnit:'seconds',targetUnit:'milliseconds',numerator:'1000',denominator:'1'},'1.234','1234'],
  [{...scaling,sourceUnit:'milliseconds',targetUnit:'seconds',numerator:'1',denominator:'1000'},'1234','1.234']];
 for(const [facts,before,after] of cases) {
  const instance=await sdk(facts);assert.equal(instance.guard((/** @type {unknown} */ x)=>x,{tool:'pay',inputShape:'raw'})(`{"value":${before}}`),`{"value":${after}}`);
 }
});
test('unknowns, stale facts, confidence and verifier refusal retain semantic write veto',async()=>{
 /** @type {[import('../src/scalar.js').ScalarFacts,unknown,()=>boolean][]} */
 const cases=[[mapping,{value:'unknown'},()=>true],[mapping,{value:'old'},()=>false],
  [{...mapping,expiresAt:1},{value:'old'},()=>true],[{...scope,recipe:'empty-null',nullMeaning:'absent'},{value:' '},()=>true],
  [scaling,{value:'125'},()=>true],[scaling,{value:1e30},()=>true]];
 for(const [facts,input,verify] of cases) {
  const instance=await sdk(facts,'repair',verify);let calls=0;
  assert.throws(()=>instance.guard((/** @type {unknown} */ x)=>{calls++;return x;},{tool:'pay',operation:'write'})(input));assert.equal(calls,0);
 }
 const instance=createAgindow({mode:'repair',selectiveRepair:{rules:[{...rule(mapping),confidence:.989}],verify:()=>true}});await instance.ready;
 assert.throws(()=>instance.guard((/** @type {unknown} */ x)=>x,{tool:'pay',operation:'write'})({value:'old'}));
});
test('observe, other routes and verifier faults preserve identity and dispatch once',async()=>{
 const input={value:'old'};
 /** @type {[import('../src/types.js').Mode,()=>boolean,string][]} */
 const cases=[['observe',()=>true,'pay'],['repair',()=>true,'other'],['repair',()=>{throw Error('fault');},'pay']];
 for(const [mode,verify,tool] of cases) {
  const instance=await sdk(mapping,mode,verify);let calls=0;
  assert.equal(instance.guard((/** @type {unknown} */ x)=>{calls++;return x;},{tool,operation:'write'})(input),input);assert.equal(calls,1);
  assert.ok(instance.recent().filter(e=>e.type==='bypass').length<=1);
 }
});
test('mapping admission requires explicit equivalence and snapshots reviewed facts',async()=>{
 /** @type {Extract<import('../src/scalar.js').ScalarFacts,{recipe:'string-map'}>} */
 const facts={...mapping,mapping:[['a','same'],['b','same']]};
 assert.throws(()=>createAgindow({selectiveRepair:{rules:[rule(facts)],verify:()=>true}}),AgindowSetupError);
 assert.throws(()=>createAgindow({selectiveRepair:{rules:[rule({...facts,equivalence:[['a','review-a']]})],verify:()=>true}}),AgindowSetupError);
 const instance=await sdk({...facts,equivalence:[['a','review-a'],['b','review-b']]});assert.deepEqual(instance.guard((/** @type {unknown} */ x)=>x,{tool:'pay'})({value:'a'}),{value:'same'});
 const pair=/** @type {[string,string]} */(['old','new']);const original={...mapping,mapping:[pair]};const frozen=await sdk(original);pair[1]='evil';
 assert.deepEqual(frozen.guard((/** @type {unknown} */ x)=>x,{tool:'pay'})({value:'old'}),{value:'new'});
});
test('empty-null only changes actual empty string and response async path applies',async()=>{
 const instance=await sdk({...scope,recipe:'empty-null',nullMeaning:'empty is absent'});
 assert.deepEqual(instance.guard((/** @type {unknown} */ x)=>x,{tool:'pay'})({value:''}),{value:null});
 const response=await sdk({...mapping,side:'response'});
 assert.deepEqual(await response.guard(async()=>({value:'old'}),{tool:'pay'})(),{value:'new'});
});

test('object numeric projections refuse lossy decoding and reads never block',async()=>{

 // This product is an unsafe integer after the scale and cannot round-trip as a JS number.
 const candidate={...scaling,sourceUnit:'dollars',targetUnit:'cents',numerator:'100',denominator:'1'};
 const precision=await sdk(candidate);const lossy={value:90071992547409.95};
 assert.throws(()=>precision.guard((/** @type {unknown} */ x)=>x,{tool:'pay',operation:'write'})(lossy));
 const reads=await sdk();const unknown={value:'new-enum'};
 assert.equal(reads.guard((/** @type {unknown} */ x)=>x,{tool:'pay',operation:'read'})(unknown),unknown);
});

test('shared scalar corpus agrees with Rust and Python and the independent verifier',async()=>{
 const {readFileSync}=await import('node:fs');
 /** @type {{id:string;facts:import('../src/scalar.js').ScalarFacts;confidence:number;before:string;after:string|null}[]} */
 const rows=JSON.parse(readFileSync(new URL('../parity/scalar-repairs.json',import.meta.url),'utf8'));
 for(const row of rows) {
  const instance=createAgindow({mode:'repair',selectiveRepair:{rules:[{...rule(row.facts),confidence:row.confidence}],verify:()=>true}});await instance.ready;
  const input=Buffer.from(row.before);
  const result=instance.guard((/** @type {Buffer} */ x)=>x,{tool:'pay',inputShape:'raw'})(input);
  assert.equal(result.toString('utf8'),row.after??row.before,row.id);
  if(row.after===null)assert.equal(result,input,row.id);
 }
});
