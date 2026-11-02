import {test} from 'node:test';
import assert from 'node:assert/strict';
import {createAgindow, AgindowSetupError, AgindowDriftError} from '../dist/esm/index.js';
/** @type {import('../src/types.js').RepairFacts} */
const facts={version:1,recipe:'rename',tool:'reviewed',side:'request',contract:'schema-2',migration:'migration-2',
 provenance:'owner-review-7',expiresAt:4102444800000,parents:[''],
 old:{key:'old',type:'string'},new:{key:'next',type:'string'}};
/** @type {import('../src/types.js').RenameRule} */
const rule={tool:'reviewed',side:'request',from:'old',to:'next',migration:'migration-2',confidence:1,facts};
/** @template T @param {T} x @returns {T} */
const identity=x=>x;
/** @param {import('../src/types.js').RenameRule} [selected] @param {import('../src/types.js').Mode} [mode] @param {import('../src/types.js').SelectiveRepairOptions['verify']} [verify] */
async function sdk(selected=rule,mode='repair',verify=()=>true){
 const instance=createAgindow({mode,selectiveRepair:{rules:[selected],verify}});await instance.ready;return instance;
}
test('versioned facts admit exact predicates independently of a verifier approving everything',async()=>{
 const instance=await sdk();
 assert.deepEqual(instance.guard(identity,{tool:'reviewed'})({old:'Ada',id:1}),{next:'Ada',id:1});
 for(const value of [{old:7},{old:null},{old:{name:'Ada'}}]) {
  assert.equal(instance.guard(identity,{tool:'reviewed'})(value),value);
 }
 const strict=await sdk(rule,'strict');
 assert.throws(()=>strict.guard(identity,{tool:'reviewed'})({old:7}),AgindowDriftError);
});
test('invalid versions, contradictory revisions, scope and constraints are named setup errors',()=>{
 for(const changed of [{version:2},{contract:''},{migration:'other'},{tool:'other'},{side:'response'},
 {provenance:''},{expiresAt:NaN},{parents:['/x']},{old:{key:'other',type:'string'}},
 {new:{key:'next',type:'number'}},{old:{key:'old',type:'guessed'}}]) {
  assert.throws(()=>createAgindow({selectiveRepair:{rules:[{...rule,facts:/** @type {import('../src/types.js').RepairFacts} */(/** @type {unknown} */({...facts,...changed}))}],verify:()=>true}}),AgindowSetupError);
 }
});
test('root metadata is retained and expiry refuses without calling verifier',async()=>{
 let called=0;
 const expired=await sdk({...rule,facts:{...facts,expiresAt:1}},'repair',()=>{called++;return true;});
 const input={old:'Ada'};
 assert.equal(expired.guard(identity,{tool:'reviewed'})(input),input);assert.equal(called,0);
 const {facts: _facts, ...legacy}=rule;
 const root=await sdk({...legacy,contract:'schema-2',provenance:'owner',expiresAt:1});
 assert.equal(root.guard(identity,{tool:'reviewed'})(input),input);
});
test('admission snapshots facts before asynchronous loading and verifier cannot rewrite authority',async()=>{
 const mutable={...facts,old:{...facts.old}};
 const pending=sdk({...rule,facts:mutable});mutable.old.type='number';mutable.expiresAt=1;
 const instance=await pending;
 assert.deepEqual(instance.guard(identity,{tool:'reviewed'})({old:'Ada'}),{next:'Ada'});
 const other={old:'Ada'};assert.equal(instance.guard(identity,{tool:'other'})(other),other);
 const observe=await sdk(rule,'observe');assert.equal(observe.guard(identity,{tool:'reviewed'})(other),other);
});

test('Node and Python share the exact admitted value predicate corpus',async()=>{
 const {readFileSync}=await import('node:fs');
 /** @type {{type:import('../src/types.js').RepairValueType,value:unknown,accepted:boolean}[]} */
 const fixtures=JSON.parse(readFileSync(new URL('../parity/repair-facts.json',import.meta.url),'utf8'));
 for(const fixture of fixtures){
  const selected={...rule,facts:{...facts,old:{key:'old',type:fixture.type},new:{key:'next',type:fixture.type}}};
  const instance=await sdk(selected);const input={old:fixture.value,id:1};
  const result=instance.guard(identity,{tool:'reviewed'})(input);
  if(fixture.accepted) assert.deepEqual(result,{next:fixture.value,id:1});
  else assert.equal(result,input);
 }
});
