import {test} from 'node:test';
import assert from 'node:assert/strict';
import {compareOperation} from '../dist/esm/index.js';
/** @type {import('../src/operation-contract.js').OperationScope} */
const scope={route:'/charges',method:'POST',side:'request',version:'v1'};
/** @param {unknown} body @param {import('../src/operation-contract.js').OperationSource['kind']} [kind] @returns {import('../src/operation-contract.js').OperationSource} */
const source=(body,kind='declared')=>({scope,kind,provenance:'reviewed',complete:true,body:JSON.stringify(body)});
test('operation-scoped declarations label nested additions directionally',async()=>{
 const baseline=source({type:'object',properties:{items:{type:'array',items:{type:'object',properties:{x:{type:'string',enum:['a']}}}}}});
 const current=source({type:'object',properties:{items:{type:'array',items:{type:'object',properties:{x:{type:['string','null']},added:{type:'number'}},required:['added']}}}});
 const result=await compareOperation({baseline,current});
 assert.equal(result.coverage,'complete');assert.ok(result.findings.some(f=>f.path==='items[].added'&&f.compatibility==='breaking'));
 const response=await compareOperation({baseline:{...baseline,scope:{...scope,side:'response'}},current:{...current,scope:{...scope,side:'response'}}});
 assert.ok(response.findings.some(f=>f.path==='items[].added'&&f.compatibility==='compatible'));
});
test('observed missing values retain unknown requiredness and route mismatch abstains',async()=>{
 const baseline=source({nested:{x:1}},'observed');const current=source({nested:{}},'observed');
 const result=await compareOperation({baseline,current});
 assert.equal(result.requiredness,'unknown');assert.equal(result.coverage,'partial');assert.equal(result.findings[0]?.compatibility,'unknown');
 assert.equal((await compareOperation({baseline,current:{...current,scope:{...scope,route:'/other'}}})).coverage,'unsupported');
});
test('unsupported, incomplete, duplicate and heterogeneous captures return no fabricated findings',async()=>{
 /** @type {[import('../src/operation-contract.js').OperationSource,import('../src/operation-contract.js').OperationSource][]} */
 const cases=[[source({type:'string',oneOf:[]}),source({type:'number'})],
  [source({items:[]},'observed'),source({items:[1,'x']},'observed')],
  [{...source({},'observed'),body:'{"x":1,"x":2}'},source({x:1},'observed')],
  [source({}),{...source({}),complete:false}]];
 for(const [baseline,current] of cases) {
  const result=await compareOperation({baseline,current});assert.notEqual(result.coverage,'complete');assert.deepEqual(result.findings,[]);
 }
 assert.deepEqual((await compareOperation({baseline:source({}),current:source({})},1)).findings,[]);
});

test('shared operation corpus agrees across native and Node/Python packaged guest',async()=>{
 const {readFileSync}=await import('node:fs');
 /** @type {{id:string;input:import('../src/operation-contract.js').OperationComparisonInput;expected:Pick<import('../src/operation-contract.js').OperationComparisonResult,'coverage'|'requiredness'> & {findings:{path:string;compatibility:string}[]}}[]} */
 const rows=JSON.parse(readFileSync(new URL('../parity/operation-contracts.json',import.meta.url),'utf8'));
 for(const row of rows) {
  const result=await compareOperation(row.input);
  assert.equal(result.coverage,row.expected.coverage,row.id);assert.equal(result.requiredness,row.expected.requiredness,row.id);
  const findings=result.findings.map(({path,compatibility})=>({path,compatibility})).sort((a,b)=>a.path.localeCompare(b.path));
  assert.deepEqual(findings,row.expected.findings.sort((a,b)=>a.path.localeCompare(b.path)),row.id);
 }
});
