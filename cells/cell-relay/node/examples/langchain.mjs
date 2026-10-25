import { DynamicStructuredTool } from '@langchain/core/tools';
import { createAgindow } from '../dist/esm/index.js';
const sdk = createAgindow(); await sdk.ready;
const existing = new DynamicStructuredTool({ name: 'search', description: 'Offline search fixture',
  schema: { type: 'object', properties: { query: { type: 'string' } }, required: ['query'] }, func: async () => 'fixture result' });
const tools = sdk.toolAdapter([existing]);
const search = tools[0]; if (search === undefined) throw Error('missing tool');
console.log(await search.invoke({ type: 'tool_call', id: 'call_fixture', name: 'search', args: { query: 'fixture' } }));
