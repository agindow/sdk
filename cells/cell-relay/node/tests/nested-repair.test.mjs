import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createAgindow, AgindowSetupError } from '../dist/esm/index.js';
/** @type {import('../src/types.js').RenameRule} */
const rule = {tool:'nested',side:'request',from:'old.key',to:'new/key',migration:'review-1',confidence:1,
 parents:['/items/0','/items/2'],contract:'schema-1',provenance:'reviewed-by-owner',expiresAt:4102444800000};
/** @param {import('../src/types.js').Mode} [mode] @param {import('../src/types.js').RenameRule} [selected] */
async function sdk(mode='repair', selected=rule) {
 const value=createAgindow({mode,selectiveRepair:{rules:[selected],verify:()=>true}}); await value.ready; return value;
}
/** @template T @param {T} value @returns {T} */
function identity(value) { return value; }
test('normal nested requests and async responses select concrete elements and report paths',async()=>{
 const value={items:[{'old.key':1},{'old.key':2},{'old.key':3}],sibling:4};
 const instance=await sdk(); let dispatched=0;
 const result=instance.guard((/** @type {typeof value} */ x)=>{dispatched++;return x;},{tool:'nested'})(value);
 assert.deepEqual(result,{items:[{'new/key':1},{'old.key':2},{'new/key':3}],sibling:4});
 assert.equal(dispatched,1); assert.equal(value.items[0]?.['old.key'],1);
 const simulated=await sdk('repair',{...rule,side:'response'});
 assert.deepEqual(simulated.simulate('nested',{recorded:value}).changes,[{path:'items[0].old\\.key',to:'items[0].new/key',kind:'renamed'},{path:'items[2].old\\.key',to:'items[2].new/key',kind:'renamed'}]);
 const response=await sdk('strict',{...rule,side:'response'});
 assert.deepEqual(await response.guard(async()=>value,{tool:'nested'})(),result);
});
test('raw nested bytes preserve numeric lexemes, escaped keys and siblings',async()=>{
 const instance=await sdk('repair',{...rule,parents:['/a~1b~0c']});
 const before=' {"a/b~c":{"old.key":9007199254740993}, "sibling":1e400} ';
 const result=instance.guard(identity,{tool:'nested',inputShape:'raw'})(before);
 assert.equal(result,before.replace('old.key','new/key'));
});
test('atomic refusal, stale facts and observe retain original identity',async()=>{
 for(const instance of [await sdk('observe'),await sdk('repair',{...rule,expiresAt:1}),await sdk()]) {
  const value={items:[{'old.key':1},{'old.key':2},{'old.key':3,'new/key':4}]};
  assert.equal(instance.guard(identity,{tool:'nested'})(value),value);
 }
 const expired=await sdk('repair',{...rule,expiresAt:1});
 const value={items:[{'old.key':1},{'old.key':2},{'old.key':3}]};
 assert.equal(expired.guard(identity,{tool:'nested'})(value),value);
});
test('invalid bounded selections and missing independent facts fail at setup',()=>{
 for(const selected of [{...rule,parents:Array(17).fill('/x')},{...rule,parents:['/x','/x']},
  {...rule,parents:['','/old.key']},{...rule,parents:['/bad~2']},{...rule,contract:''}]) {
  assert.throws(()=>createAgindow({selectiveRepair:{rules:[selected],verify:()=>true}}),AgindowSetupError);
 }
});

test('shared Rust/Node/Python corpus preserves bytes or original raw identity',async()=>{
 const {readFileSync}=await import('node:fs');
 /** @type {{id:string;source:string;target:string;parents:string[];before:string;after:string|null}[]} */
 const fixtures=JSON.parse(readFileSync(new URL('../parity/nested-renames.json',import.meta.url),'utf8'));
 for(const fixture of fixtures) {
  const instance=await sdk('repair',{...rule,from:fixture.source,to:fixture.target,parents:fixture.parents});
  const input=Buffer.from(fixture.before);
  const result=instance.guard(identity,{tool:'nested',inputShape:'raw'})(input);
  if(fixture.after===null) assert.equal(result,input,fixture.id);
  else assert.equal(result.toString('utf8'),fixture.after,fixture.id);
 }
});
test('exhausted capture and work preserve identity and one dispatch',async()=>{
 for(const options of [{work:1},{}]) {
  const instance=createAgindow({mode:'repair',...options,selectiveRepair:{rules:[rule],verify:()=>true}});await instance.ready;
  const input=Buffer.from(options.work===1?'{"items":[{"old.key":1},{},{"old.key":2}]}':'{"pad":"'+'x'.repeat(1_048_576)+'"}');
  let calls=0;
  const result=instance.guard((/** @type {Buffer} */ x)=>{calls++;return x;},{tool:'nested',inputShape:'raw'})(input);
  assert.equal(result,input);assert.equal(calls,1);
  assert.equal(instance.recent().filter(e=>e.type==='bypass').length,1);
 }
});
test('nested repair retains upstream exception identity without retrying writes',async()=>{
 const instance=await sdk();const error=Error('upstream');let calls=0;
 assert.throws(()=>instance.guard((/** @type {unknown} */ _value)=>{calls++;throw error;},{tool:'nested',operation:'write'})({items:[{'old.key':1},{},{'old.key':3}]}),value=>value===error);
 assert.equal(calls,1);
});
test('versioned facts constrain the entire concrete nested batch',async()=>{
 const facts={version:/** @type {const} */(1),recipe:/** @type {const} */('rename'),tool:rule.tool,side:rule.side,
  contract:'schema-1',migration:rule.migration,provenance:'reviewed-by-owner',expiresAt:4102444800000,parents:rule.parents??[],
  old:{key:rule.from,type:/** @type {const} */('number')},new:{key:rule.to,type:/** @type {const} */('number')}};
 const instance=await sdk('repair',{...rule,facts});
 const value={items:[{'old.key':1},{},{'old.key':3}]};
 assert.deepEqual(instance.guard(identity,{tool:'nested'})(value),{items:[{'new/key':1},{},{'new/key':3}]});
 const mismatch={items:[{'old.key':1},{},{'old.key':'wrong type'}]};
 assert.equal(instance.guard(identity,{tool:'nested'})(mismatch),mismatch);
});
