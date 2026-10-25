import { afterEach, expect, it, vi } from 'vitest';
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { resolveConfig } from '../src/config.js';
const directories: string[] = [];
afterEach(() => { for (const path of directories.splice(0)) rmSync(path, { recursive: true, force: true }); vi.restoreAllMocks(); });
function fixture(text: string): string {
  const root = mkdtempSync(join(tmpdir(), 'agw-config-')); directories.push(root);
  writeFileSync(join(root, '.agindowrc'), text); mkdirSync(join(root, 'child')); return root;
}
it('resolves explicit > environment > nearest file > defaults without credentials', () => {
  const root = fixture('{"mode":"strict","buffer":3,"apiKey":"local-test"}');
  const env = { AGINDOW_MODE: 'repair' };
  expect(resolveConfig({ mode: 'observe' }, { cwd: join(root, 'child'), env }).mode).toBe('observe');
  expect(resolveConfig({}, { cwd: root, env }).mode).toBe('repair');
  expect(resolveConfig({}, { cwd: root, env: {} })).toMatchObject({ mode: 'strict', buffer: 3 });
  rmSync(join(root, '.agindowrc'));
  expect(resolveConfig({}, { cwd: root, env: {} })).toMatchObject({ mode: 'observe', failClosed: true, spans: true, buffer: 100 });
});
it('warns and ignores corrupt or invalid nearest files without searching past them', () => {
  const root = fixture('{"mode":"strict"}');
  const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
  for (const value of ['{', '{"buffer":-1}', 'null', '{"mode":"bogus"}']) {
    writeFileSync(join(root, 'child', '.agindowrc'), value);
    expect(resolveConfig({}, { cwd: join(root, 'child'), env: {} }).mode).toBe('observe');
  }
  expect(warn).toHaveBeenCalledTimes(4);
});
it('validates typed policy and observability options at setup', () => {
  const root = fixture('{}');
  for (const invalid of [{ buffer: -1 }, { buffer: Infinity }, { spans: 'yes' }, { denyTools: [1] },
    { allowedTools: 'vendor.*' }, { endpoint: 'not a URL' }, { sync: { drift: -1 } }]) {
    expect(() => resolveConfig(invalid as never, { cwd: root, env: {} })).toThrow();
  }
});

it('defaults to local-only observe without a key, endpoint, sidecar or synchronization', () => {
  const root = fixture('{}');
  const config = resolveConfig({}, { cwd: root, env: {} });
  expect(config).toEqual({ mode: 'observe', failClosed: true, spans: true, buffer: 100, allowedTools: '*' });
  for (const key of ['apiKey', 'endpoint', 'sidecar', 'sync']) expect(Object.hasOwn(config, key)).toBe(false);
});
