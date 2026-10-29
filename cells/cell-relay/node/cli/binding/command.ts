import { closeSync, constants, fstatSync, lstatSync, openSync, readSync } from 'node:fs';
import { bindSource, resolveBinding } from './index.js';
import { isGrant, isSelection, record } from './input.js';
import { abstain } from './types.js';
import type { BindingResult } from './types.js';
/** Local grants are explicit maintainer assertions, not authenticated hosted grants. */
export function bindingCommand(command: 'bind-source' | 'resolve-source', args: readonly string[]): number {
  let result: BindingResult = abstain('invalid_selection');
  const path = args[0];
  if (path !== undefined && args.length === 1) try {
    if (!lstatSync(path).isFile()) throw Error('request must be a file');
    const fd = openSync(path, constants.O_RDONLY | constants.O_NOFOLLOW);
    let input: unknown;
    try {
      if (!fstatSync(fd).isFile() || fstatSync(fd).size > 1024 * 1024) throw Error('request limit');
      const bytes = Buffer.alloc(1024 * 1024 + 1);
      const size = readSync(fd, bytes, 0, bytes.length, 0);
      if (size > 1024 * 1024) throw Error('request limit');
      input = JSON.parse(bytes.subarray(0, size).toString('utf8'));
    } finally { closeSync(fd); }
    if (record(input) && isGrant(input['grant'])) {
      if (command === 'bind-source' && isSelection(input['selection'])) result = bindSource(input['grant'], input['selection']);
      if (command === 'resolve-source') result = resolveBinding(input['grant'], input['manifest']);
    }
  } catch { result = abstain('invalid_selection'); }
  console.log(JSON.stringify(result));
  return result.status === 'bound' ? 0 : 1;
}
