import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createAgindow, AgindowDriftError, AgindowSetupError } from '../dist/esm/index.js';
/** @type {import('../src/types.js').RenameRule} */
const rule={tool:'contacts.create',side:'request',from:'old_name',to:'display_name',migration:'crm-2026-10-reviewed',confidence:1};
/** @param {import('../src/types.js').RepairCheck} check */
const approved=check=>JSON.parse(check.before).old_name===JSON.parse(check.after).display_name;
/**
 * @param {import('../src/types.js').Mode} [mode]
 * @param {(check: import('../src/types.js').RepairCheck) => unknown} [verify]
 * @param {import('../src/types.js').Options} [overrides]
 */
async function instance(mode='repair',verify=approved,overrides={}){
 const sdk=createAgindow({mode,selectiveRepair:{rules:[rule],verify:/** @type {import('../src/types.js').SelectiveRepairOptions['verify']} */(verify)},...overrides});await sdk.ready;return sdk;
}
/** @template T @param {T} value @returns {T} */
function identity(value){return value;}
test('default and observe preserve the same input object, while opt-in repairs normal requests',async()=>{
 const input={old_name:'Ada',untouched:{id:7}};
 const plain=createAgindow({mode:'repair'});await plain.ready;
 assert.equal(plain.guard(identity,{tool:rule.tool})(input),input);
 const observe=await instance('observe');assert.equal(observe.guard(identity,{tool:rule.tool})(input),input);
 const sdk=await instance();const result=sdk.guard(identity,{tool:rule.tool,operation:'write'})(input);
 assert.deepEqual(result,{display_name:'Ada',untouched:{id:7}});assert.equal(input.old_name,'Ada');
 assert.equal(sdk.recent().filter(e=>e.type==='repair').length,1);
});
test('response scope and async guard apply only reviewed verified renames',async()=>{
 const sdk=await instance('strict',approved,{selectiveRepair:{rules:[{...rule,side:'response'}],verify:approved}});
 assert.deepEqual(await sdk.guard(async()=>({old_name:'Grace',id:1}),{tool:rule.tool})(),{display_name:'Grace',id:1});
 const input={old_name:'Ada'};assert.equal(sdk.guard(identity,{tool:'other'})(input),input);
});
test('refused verification, low confidence and collisions never mutate, strict refuses',async()=>{
 const input={old_name:'Ada'};
 const refused=await instance('repair',()=>false);assert.equal(refused.guard(identity,{tool:rule.tool})(input),input);
 const strict=await instance('strict',()=>false);let called=0;
 assert.throws(()=>strict.guard((/** @type {unknown} */ x)=>{called++;return x;},{tool:rule.tool})(input),AgindowDriftError);assert.equal(called,0);
 const collision={old_name:'Ada',display_name:'Different'};const sdk=await instance();assert.equal(sdk.guard(identity,{tool:rule.tool})(collision),collision);
 const low=await instance('repair',approved,{selectiveRepair:{rules:[{...rule,confidence:.96}],verify:approved}});
 assert.equal(low.guard(identity,{tool:rule.tool})(input),input);
});
test('verifier faults and async verifier results bypass once, original outcome remains',async()=>{
 for(const verify of [()=>{throw Error('verifier failed');},async()=>true]){
  const sdk=await instance('repair',verify);const input={old_name:'Ada'};
  assert.equal(sdk.guard(identity,{tool:rule.tool})(input),input);
  assert.equal(sdk.recent().filter(e=>e.type==='bypass').length,1);
 }
});
test('rejects invalid rules and unsupported guest audit combination at setup',()=>{
 assert.throws(()=>createAgindow({selectiveRepair:{rules:[rule,rule],verify:approved}}),AgindowSetupError);
 assert.throws(()=>createAgindow({selectiveRepair:{rules:[{...rule,from:'nested.field'}],verify:approved}}),AgindowSetupError);
 assert.throws(()=>createAgindow({audit:{tenant:'t'},selectiveRepair:{rules:[rule],verify:approved}}),AgindowSetupError);
});
test('statistical semantic writes retain their block and never invoke repair verification',async()=>{
 let verified=0;let dispatched=0;
 const sdk=await instance('repair',()=>{verified++;return true;},{selection:{tenant:'t',connection:'c',vendor:'crm',operation:rule.tool,segment:'s',
  fields:[{side:'request',path:'/amount',enumeration:true,baseline:Array(128).fill('{"amount":1}'),live:Array(128).fill('{"amount":100}')}]}});
 assert.throws(()=>sdk.guard((/** @type {unknown} */ x)=>{dispatched++;return x;},{tool:rule.tool,operation:'write'})({old_name:'Ada',amount:100}),AgindowDriftError);
 assert.equal(verified,0);assert.equal(dispatched,0);
});
test('unchanged payloads preserve identity and opaque capture never invokes getters',async()=>{
 const sdk=await instance();const unchanged={display_name:'Ada',id:1};
 assert.equal(sdk.guard(identity,{tool:rule.tool})(unchanged),unchanged);
 let getters=0;const opaque={get old_name(){getters++;return 'Ada';}};
 assert.equal(sdk.guard(identity,{tool:rule.tool})(opaque),opaque);assert.equal(getters,0);
 assert.equal(sdk.recent().filter(e=>e.type==='bypass').length,1);
});
test('normal configure mode toggles reviewed repair without rebuilding the guarded callable',async()=>{
 const sdk=await instance('observe');const fn=sdk.guard(identity,{tool:rule.tool});const value={old_name:'Ada'};
 assert.equal(fn(value),value);sdk.configure({mode:'repair'});assert.deepEqual(fn(value),{display_name:'Ada'});
 sdk.configure({mode:'observe'});assert.equal(fn(value),value);
});
test('unverified shape writes retain shared Rust passthrough policy and original upstream exceptions',async()=>{
 const sdk=await instance('repair',()=>false);let dispatched=0;const input={old_name:'Ada'};
 assert.equal(sdk.guard((/** @type {unknown} */ x)=>{dispatched++;return x;},{tool:rule.tool,operation:'write'})(input),input);assert.equal(dispatched,1);
 const optedOut=await instance('repair',()=>false,{failClosed:false});
 assert.equal(optedOut.guard(identity,{tool:rule.tool,operation:'write'})(input),input);
 const approvedSdk=await instance();const error=Error('vendor error');
 assert.throws(()=>approvedSdk.guard((/** @type {unknown} */ _input)=>{throw error;},{tool:rule.tool})(input),value=>value===error);
});
