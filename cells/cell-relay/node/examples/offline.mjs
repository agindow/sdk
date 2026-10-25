import { readFileSync } from 'node:fs';
import { createAgindow } from '../dist/esm/index.js';
/** @type {{contracts: import('../dist/esm/index.js').SandboxContract[]}} */
const fixture = JSON.parse(readFileSync(new URL('./nango-contracts.json', import.meta.url), 'utf8'));
const contract = fixture.contracts[0]; if (contract === undefined) throw Error('missing demonstration contract');
const sdk = createAgindow({ mode: 'repair' }); await sdk.ready;
const original = { ok: true }; const run = sdk.guard(() => original);
if (run() !== original) throw Error('identity lost');
console.log(JSON.stringify(sdk.simulate(contract.tool, { contract, args: { source: 'fictional-token' },
  recorded: { status: 'active' }, mode: 'repair', operation: 'write' }), null, 2));
