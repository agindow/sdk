import { expect, it } from 'vitest';
import { mkdtemp, writeFile, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { jsonLines } from '../cli/rotation/input.js';
async function collect(values: AsyncIterable<unknown>): Promise<unknown[]> { const result: unknown[] = []; for await (const value of values) result.push(value); return result; }
it('bounds unterminated lines across chunks and rejects malformed records', async () => {
 const dir = await mkdtemp(join(tmpdir(), 'rotation-'));
 try {
  const file = join(dir, 'events.ndjson');
  await writeFile(file, 'x'.repeat(100000));
  await expect(collect(jsonLines(file))).rejects.toThrow('too large');
  await writeFile(file, '{"id":1}\ninvalid');
  await expect(collect(jsonLines(file))).rejects.toThrow();
  await writeFile(file, '{"id":1}\n{"id":2}');
  expect(await collect(jsonLines(file))).toEqual([{id:1},{id:2}]);
 } finally { await rm(dir, { recursive:true, force:true }); }
});
