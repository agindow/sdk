import { createAgindow } from 'agindow';

const sdk = createAgindow({ mode: 'observe', buffer: 16 });
await sdk.ready;

const original = { ok: true };
const run = sdk.guard(() => original, { tool: 'example.read', operation: 'read' });
const result = run();

if (result !== original) {
  throw new Error('expected return-by-reference on the no-repair path');
}

console.log(JSON.stringify({ ready: true, result }, null, 2));
