import { afterEach, expect, it, vi } from 'vitest';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { bindingCommand } from '../cli/binding/command.js';
const roots: string[] = [];
afterEach(() => { vi.restoreAllMocks(); for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true }); });
it('rejects malformed, excessive and untrusted request shapes as JSON refusals', () => {
  const root = mkdtempSync(join(tmpdir(), 'agw-bind-command-')); roots.push(root);
  const path = join(root, 'request.json'); const output = vi.spyOn(console, 'log').mockImplementation(() => {});
  for (const text of ['null', '{', '{}', '{"grant":{},"selection":{}}', 'x'.repeat(1024 * 1024 + 1)]) {
    writeFileSync(path, text);
    expect(bindingCommand('bind-source', [path])).toBe(1);
    expect(JSON.parse(String(output.mock.calls.at(-1)?.[0]))).toMatchObject({ status: 'abstained' });
  }
  expect(bindingCommand('bind-source', [])).toBe(1);
});
