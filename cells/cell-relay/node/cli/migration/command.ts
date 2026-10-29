import { closeSync, constants, fstatSync, lstatSync, openSync, readSync } from 'node:fs';
import { generateSourcePatch } from './index.js';
import { refused } from './types.js';
import type { MigrationResult } from './types.js';
/** Explicit local export; stdout persistence is caller-owned, never an in-place edit. */
export async function migrationCommand(args: readonly string[]): Promise<number> {
  let result: MigrationResult = refused('invalid_selection');
  const [path, flag, asOf] = args;
  if (args.length === 3 && path !== undefined && flag === '--as-of' && asOf !== undefined
    && /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{3})?Z$/.test(asOf) && Number.isFinite(Date.parse(asOf))) try {
    if (!lstatSync(path).isFile()) throw Error('request must be a file');
    const fd = openSync(path, constants.O_RDONLY | constants.O_NOFOLLOW);
    let input: unknown;
    try {
      if (!fstatSync(fd).isFile() || fstatSync(fd).size > 64 * 1024) throw Error('request limit');
      const bytes = Buffer.alloc(64 * 1024 + 1);
      const size = readSync(fd, bytes, 0, bytes.length, 0);
      if (size > 64 * 1024) throw Error('request limit');
      input = JSON.parse(bytes.subarray(0, size).toString('utf8'));
    } finally { closeSync(fd); }
    result = await generateSourcePatch(input, Date.parse(asOf));
  } catch { result = refused('invalid_selection'); }
  console.log(JSON.stringify(result));
  return result.status === 'refused' ? 1 : 0;
}
