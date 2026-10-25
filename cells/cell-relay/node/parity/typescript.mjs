import { readFileSync } from 'node:fs';
import { createAgindow, AgindowDriftError } from '../dist/esm/index.js';
const pass = { halt: false, bypassed: false, class: null, findings: 0, detectors: 0, malformed: false };
const halt = { ...pass, halt: true, findings: 1, class: /** @type {const} */ ('semantic') };
/** @type {{ cases: {id:string; scenario:string; mode:import('../dist/esm/index.js').Mode; operation:import('../dist/esm/index.js').Operation; args:unknown; recorded:unknown}[] }} */
const definition = JSON.parse(readFileSync(0, 'utf8'));
/** @param {'request' | 'response'} side @returns {import('../dist/esm/index.js').StatisticalSelection} */
const selection = side => ({ tenant: 't', connection: 'c', vendor: 'v', operation: 'op', segment: 's',
  fields: [{ side, path: '/value', baseline: Array(128).fill('{"value":1}'),
    live: Array(128).fill('{"value":100}'), enumeration: true }] });
const rows = [];
for (const fixture of definition.cases) {
  const sdk = fixture.scenario.startsWith('wasm_')
    ? createAgindow({ mode: fixture.mode, selection: selection(fixture.scenario === 'wasm_response' ? 'response' : 'request'),
      work: fixture.scenario === 'wasm_budget' ? 0 : 200_000_000 })
    : createAgindow({ mode: fixture.mode }, async () => ({ begin() {
    if (fixture.scenario === 'fault') throw Error('fixture');
    return { request: fixture.scenario === 'halt_request' ? halt : pass,
      finish: () => fixture.scenario === 'halt_response' ? halt : pass, close() {} };
  } }));
  await sdk.ready;
  let calls = 0; let sameValue = false; let sameError = false; let retainedOutcome = false;
  let sameArgs = false; let sameThis = false; let isAsync = false;
  const receiver = {}; const originalError = Error('original fixture exception');
  /** @type {string | null} */ let code = null;
  /** @type {string | null} */ let driftClass = null;
  const fn = sdk.guard(/** @this {object} */ function(/** @type {unknown} */ value) {
    calls++;
    sameArgs = value === fixture.args; sameThis = this === receiver;
    if (fixture.scenario === 'error') throw originalError;
    return fixture.scenario === 'async' ? Promise.resolve(fixture.recorded) : fixture.recorded;
  }, { tool: 'op', operation: fixture.operation });
  try { const result = fn.call(receiver, fixture.args); isAsync = result instanceof Promise; sameValue = await result === fixture.recorded; }
  catch (error) {
    sameError = error === originalError;
    if (error instanceof AgindowDriftError) { code = error.code; driftClass = error.driftClass; retainedOutcome = error.outcome === fixture.recorded; }
  }
  rows.push({ id: fixture.id, calls, sameValue, sameError, retainedOutcome, code, driftClass, sameArgs, sameThis, isAsync,
    bypasses: sdk.recent().filter(event => event.type === 'bypass').length });
}
console.log(JSON.stringify(rows));
