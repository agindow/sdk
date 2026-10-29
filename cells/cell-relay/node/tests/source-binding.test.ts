import { afterEach, expect, it, vi } from 'vitest';
import { mkdtempSync, mkdirSync, realpathSync, rmSync, symlinkSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { execFileSync } from 'node:child_process';
import { bindingCommand } from '../cli/binding/command.js';
import { bindSource, resolveBinding } from '../cli/binding/index.js';
import type { CheckoutGrant, BindingSelection } from '../cli/binding/types.js';
const roots: string[] = [];
const source = `import vendor from 'unused';
export function serialize(input: { source: string }) { return { source: input.source }; }
export function consume(result: { source: string }) { return result.source; }
const unrelated = { source: 'unrelated' };
throw Error('never execute');`;
function fixture(text = source): CheckoutGrant {
  const root = realpathSync(mkdtempSync(join(tmpdir(), 'agw-binding-'))); roots.push(root);
  mkdirSync(join(root, 'packages/client'), { recursive: true });
  writeFileSync(join(root, 'packages/client/api.ts'), text);
  const git = (args: string[]) => execFileSync('git', ['-C', root, ...args], { encoding: 'utf8' }).trim();
  git(['init', '-q']); git(['add', '.']);
  git(['-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.test', '-c', 'commit.gpgsign=false', 'commit', '-qm', 'fixture']);
  return { root, tenantId: 'tenant-a', repositoryId: 'repo-a', baseSha: git(['rev-parse', 'HEAD']), actorId: 'owner-a', authorityRevision: 'grant-1' };
}
const selection: BindingSelection = { tenantId: 'tenant-a', repositoryId: 'repo-a', operation: 'customer.create', side: 'request',
  file: 'packages/client/api.ts', symbol: 'serialize', field: 'source', bindingVersion: 1 };
afterEach(() => { vi.restoreAllMocks(); for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true }); });
it('binds a monorepo serializer to committed bytes and exact AST key, independently of vendor/name hints', () => {
  const grant = fixture(); const result = bindSource(grant, selection);
  expect(result.status).toBe('bound');
  if (result.status !== 'bound') throw Error('expected binding');
  expect(result.manifest).toMatchObject({ ...selection, baseSha: grant.baseSha, actorId: 'owner-a', authorityRevision: 'grant-1', formatVersion: 1 });
  expect(result.manifest.fileDigest).toMatch(/^[a-f0-9]{64}$/);
  expect(source.slice(result.manifest.span.start, result.manifest.span.end)).toBe('source');
  expect(result.manifest.span.line).toBe(2);
  expect(resolveBinding(grant, result.manifest)).toEqual(result);
  expect(resolveBinding(grant, { ...result.manifest, operation: 'other' }).status).toBe('abstained');
});
it('binds response access only within the explicitly selected function', () => {
  const result = bindSource(fixture(), { ...selection, symbol: 'consume', side: 'response' });
  expect(result.status).toBe('bound');
  if (result.status === 'bound') expect(source.slice(result.manifest.span.start, result.manifest.span.end)).toBe('source');
});
it.each([
  `function serialize(input) { const alias = input; return { source: alias.source }; }`,
  `function serialize(input) { return { ['source']: input.source }; }`,
  `function serialize(input) { return { source: input.source, source: input.other }; }`,
  `function serialize(input) { return { ...input, source: input.source }; }`,
  `import { serialize } from 'vendor'; function outer(serialize) { return serialize.source; }`,
  `function serialize(input) { return { source: (() => input.source)() }; }`,
  `function serialize(input) { return { source: other.source }; }`,
])('abstains with locations and manual path for unsupported or ambiguous syntax: %s', text => {
  const result = bindSource(fixture(text), selection);
  expect(result).toMatchObject({ status: 'abstained', manual: expect.any(String) });
  if (result.status === 'abstained') expect(result.locations.length).toBeGreaterThan(0);
});
it('refuses stale commits, dirty bytes and altered binding versions', () => {
  const grant = fixture(); const result = bindSource(grant, selection);
  expect(bindSource({ ...grant, baseSha: '0'.repeat(40) }, selection)).toMatchObject({ status: 'abstained', reason: 'stale_base' });
  if (result.status !== 'bound') throw Error('binding missing');
  expect(resolveBinding(grant, { ...result.manifest, bindingVersion: 2 }).status).toBe('abstained');
  writeFileSync(join(grant.root, selection.file), source + '\n// edit');
  expect(resolveBinding(grant, result.manifest)).toMatchObject({ status: 'abstained', reason: 'stale_source' });
});
it('refuses another tenant or repository before reading source', () => {
  const grant = fixture();
  for (const mismatch of [{ tenantId: 'tenant-b' }, { repositoryId: 'repo-b' }]) {
    expect(bindSource(grant, { ...selection, ...mismatch })).toMatchObject({ status: 'abstained', reason: 'unauthorized' });
  }
});
it.each(['../outside.ts', '/tmp/outside.ts', 'packages/../api.ts', 'node_modules/api.ts', 'vendor/api.ts', 'dist/api.ts', 'src/generated/api.ts', 'api.generated.ts'])('refuses unsafe or excluded paths: %s', file => {
  expect(bindSource(fixture(), { ...selection, file })).toMatchObject({ status: 'abstained', reason: 'unsafe_path' });
});
it('refuses symlink directories, symlink roots and generated markers', () => {
  const grant = fixture();
  symlinkSync(join(grant.root, 'packages/client'), join(grant.root, 'linked'));
  expect(bindSource(grant, { ...selection, file: 'linked/api.ts' })).toMatchObject({ status: 'abstained', reason: 'unsafe_path' });
  const link = grant.root + '-link'; roots.push(link); symlinkSync(grant.root, link);
  expect(bindSource({ ...grant, root: link }, selection)).toMatchObject({ status: 'abstained', reason: 'unsafe_path' });
  expect(bindSource(fixture('// @generated\n' + source), selection)).toMatchObject({ status: 'abstained', reason: 'excluded_source' });
});
it('enforces byte, syntax and selection bounds', () => {
  expect(bindSource(fixture('x'.repeat(1024 * 1024 + 1)), selection)).toMatchObject({ status: 'abstained', reason: 'source_limit' });
  expect(bindSource(fixture('export const ='), selection)).toMatchObject({ status: 'abstained', reason: 'unsupported_syntax' });
  expect(bindSource(fixture(), { ...selection, bindingVersion: 0 }).status).toBe('abstained');
});
it('supports explicit const arrows and literal keys without selecting identical unrelated keys', () => {
  const text = `const unrelated = { source: 'wrong' }; export const serialize = (input: Record<string, string>) => ({ 'source': input.source });`;
  const result = bindSource(fixture(text), selection);
  expect(result.status).toBe('bound');
  if (result.status === 'bound') expect(text.slice(result.manifest.span.start, result.manifest.span.end)).toBe("'source'");
});
it.each([
  'function consume(result) { return result[field]; }',
  'function consume(result) { const alias = result; return alias.source; }',
  'function consume(result) { function nested(result) { return result.source; } return result.source; }',
  'function consume(result) { return other.source; }',
  'function consume(result) { return result?.source; }',
])('refuses dynamic or shadowed response access: %s', text => {
  expect(bindSource(fixture(text), { ...selection, side: 'response', symbol: 'consume' })).toMatchObject({ status: 'abstained', reason: 'unsupported_syntax' });
});
it('rechecks actor authority, all digest-bound fields and stored spans rather than trusting a manifest', () => {
  const grant = fixture(); const result = bindSource(grant, selection);
  if (result.status !== 'bound') throw Error('missing fixture binding');
  for (const change of [{ actorId: 'other' }, { authorityRevision: 'revoked' }]) {
    expect(resolveBinding({ ...grant, ...change }, result.manifest)).toMatchObject({ status: 'abstained', reason: 'binding_mismatch' });
  }
  for (const change of [{ fileDigest: '0'.repeat(64) }, { bindingId: '0'.repeat(64) }, { formatVersion: 2 },
    { span: { ...result.manifest.span, start: 0 } }]) {
    expect(resolveBinding(grant, { ...result.manifest, ...change })).toMatchObject({ status: 'abstained', reason: 'binding_mismatch' });
  }
});
it('refuses an actual symlink escape and an uncommitted file even if its syntax is valid', () => {
  const grant = fixture(); const outside = fixture();
  symlinkSync(join(outside.root, selection.file), join(grant.root, 'escape.ts'));
  expect(bindSource(grant, { ...selection, file: 'escape.ts' })).toMatchObject({ status: 'abstained', reason: 'unsafe_path' });
  writeFileSync(join(grant.root, 'untracked.ts'), source);
  expect(bindSource(grant, { ...selection, file: 'untracked.ts' })).toMatchObject({ status: 'abstained', reason: 'unavailable' });
});

it('round-trips local CLI requests without modifying the checkout', () => {
  const grant = fixture(); const path = join(grant.root, 'request.json');
  const output = vi.spyOn(console, 'log').mockImplementation(() => {});
  writeFileSync(path, JSON.stringify({ grant, selection }));
  expect(bindingCommand('bind-source', [path])).toBe(0);
  const result: unknown = JSON.parse(String(output.mock.calls.at(-1)?.[0]));
  if (result === null || typeof result !== 'object' || !('manifest' in result)) throw Error('missing CLI manifest');
  writeFileSync(path, JSON.stringify({ grant, manifest: result.manifest }));
  expect(bindingCommand('resolve-source', [path])).toBe(0);
  expect(JSON.parse(String(output.mock.calls.at(-1)?.[0]))).toEqual(result);
  expect(readFileSync(join(grant.root, selection.file), 'utf8')).toBe(source);
});
