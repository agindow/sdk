import { afterEach, expect, it, vi } from 'vitest';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { migrationCommand } from '../cli/migration/command.js';
import { migrationFixture, NOW, requestSource } from './support/source-migration.js';
const roots: string[] = [];
afterEach(() => { vi.restoreAllMocks(); for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true }); });
it('exports JSON patch/receipt using an explicit UTC clock and never writes customer files', async () => {
  const input = await migrationFixture(roots); const request = join(input.checkout.root, 'request.json');
  writeFileSync(request, JSON.stringify(input)); const output = vi.spyOn(console, 'log').mockImplementation(() => {});
  expect(await migrationCommand([request, '--as-of', new Date(NOW).toISOString()])).toBe(0);
  expect(JSON.parse(String(output.mock.calls.at(-1)?.[0]))).toMatchObject({ status: 'generated', patch: expect.stringContaining('payment_method'), receipt: { validation: 'unvalidated' } });
  expect(readFileSync(join(input.checkout.root, 'api.ts'), 'utf8')).toBe(requestSource);
});
it('refuses malformed or excessive requests and missing/non-UTC time without echoing private content', async () => {
  const root = mkdtempSync(join(tmpdir(), 'agw-migration-command-')); roots.push(root);
  const request = join(root, 'request.json'); const output = vi.spyOn(console, 'log').mockImplementation(() => {});
  for (const text of ['null', '{', '{}', 'x'.repeat(64 * 1024 + 1)]) {
    writeFileSync(request, text);
    expect(await migrationCommand([request, '--as-of', new Date(NOW).toISOString()])).toBe(1);
    expect(JSON.parse(String(output.mock.calls.at(-1)?.[0]))).toMatchObject({ status: 'refused' });
  }
  expect(await migrationCommand([request])).toBe(1);
  expect(await migrationCommand([request, '--as-of', '2026-10-20T12:00:00'])).toBe(1);
});
