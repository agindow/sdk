import { afterEach, expect, it, vi } from 'vitest';
import { readFileSync, rmSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { execFileSync } from 'node:child_process';
import { canonicalJson, sourceRepairDigest } from 'contracts';
import { generateSourcePatch } from '../cli/migration/index.js';
import { digest } from '../cli/binding/checkout.js';
import { migrationFixture, NOW, requestSource } from './support/source-migration.js';
const roots: string[] = [];
afterEach(() => { vi.restoreAllMocks(); for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true }); });
function apply(root: string, patch: string): string {
  // Git is an independent parser; applying is restricted to this disposable fixture.
  execFileSync('git', ['-C', root, 'apply', '--check', '-'], { input: patch });
  execFileSync('git', ['-C', root, 'apply', '-'], { input: patch });
  return readFileSync(join(root, 'api.ts'), 'utf8');
}
it('exports a deterministic parseable minimal request patch, preserving model names, aliases, comments and CRLF', async () => {
  const input = await migrationFixture(roots); const fetch = vi.spyOn(globalThis, 'fetch').mockRejectedValue(Error('network forbidden'));
  const result = await generateSourcePatch(input, NOW);
  expect(result.status).toBe('generated');
  if (result.status !== 'generated') throw Error('missing patch');
  expect(await generateSourcePatch(input, NOW)).toEqual(result);
  expect(readFileSync(join(input.checkout.root, 'api.ts'), 'utf8')).toBe(requestSource);
  const expected = requestSource.replace('source /* keep */:', 'payment_method /* keep */:');
  expect(result.receipt).toMatchObject({ recipe: 'js-ts-top-level-rename', recipeVersion: 1, validation: 'unvalidated',
    baseSha: input.checkout.baseSha, bindingId: input.binding.bindingId, originalDigest: digest(requestSource), baseDigest: digest(requestSource),
    resultDigest: digest(expected), patchDigest: digest(result.patch), factDigest: digest(canonicalJson(input.evidence.facts[0])), evidenceDigest: input.evidence.digest });
  expect(apply(input.checkout.root, result.patch)).toBe(expected); expect(fetch).not.toHaveBeenCalled();
});
it('changes only the selected response access and preserves a shadowed unrelated local model field', async () => {
  const source = `import { source as model } from 'unused';\nfunction other(result) { return result.source; }\nexport const consume = (result: { source: string }) => result.source;\n`;
  const input = await migrationFixture(roots, source, 'response'); const result = await generateSourcePatch(input, NOW);
  expect(result.status).toBe('generated');
  if (result.status === 'generated') expect(apply(input.checkout.root, result.patch)).toBe(source.replace('=> result.source', '=> result.payment_method'));
});
it.each(["'source'", '"source"'])('preserves literal key quote style: %s', async key => {
  const source = `function serialize(input) { return { ${key}: input.source }; }\n`;
  const input = await migrationFixture(roots, source); const result = await generateSourcePatch(input, NOW);
  expect(result.status).toBe('generated');
  if (result.status === 'generated') expect(apply(input.checkout.root, result.patch)).toBe(source.replace(key, key.replace('source', 'payment_method')));
});
it('refuses a destination collision without writing', async () => {
  const source = 'function serialize(input) { return { source: input.source, payment_method: input.other }; }';
  const input = await migrationFixture(roots, source);
  expect(await generateSourcePatch(input, NOW)).toMatchObject({ status: 'refused', reason: 'collision' });
  expect(readFileSync(join(input.checkout.root, 'api.ts'), 'utf8')).toBe(source);
});
it('refuses dirty targets, stale commits and tampered spans instead of reporting no-op', async () => {
  const input = await migrationFixture(roots);
  expect(await generateSourcePatch({ ...input, checkout: { ...input.checkout, baseSha: '0'.repeat(40) } }, NOW)).toMatchObject({ status: 'refused', reason: 'stale_base' });
  expect(await generateSourcePatch({ ...input, binding: { ...input.binding, span: { ...input.binding.span, start: 0 } } }, NOW)).toMatchObject({ status: 'refused', reason: 'binding_mismatch' });
  writeFileSync(join(input.checkout.root, 'api.ts'), requestSource.replace('source /* keep */:', 'payment_method /* keep */:'));
  expect(await generateSourcePatch(input, NOW)).toMatchObject({ status: 'refused', reason: 'stale_source' });
});
it('returns a typed deterministic no-op only with a fresh binding to the committed migrated field', async () => {
  const source = 'function serialize(input) { return { payment_method: input.source }; }';
  const input = await migrationFixture(roots, source, 'request', 'payment_method');
  expect(await generateSourcePatch(input, NOW)).toMatchObject({ status: 'noop', reason: 'already_present' });
  expect(await generateSourcePatch(input, NOW)).toEqual(await generateSourcePatch(input, NOW));
  expect(readFileSync(join(input.checkout.root, 'api.ts'), 'utf8')).toBe(source);
});
it('validates evidence and explicit generation authority before reading checkout files', async () => {
  const input = await migrationFixture(roots); const checkout = { ...input.checkout, root: '/nonexistent/never-read' };
  for (const grant of [{ ...input.grant, evidenceDigest: '0'.repeat(64) }, { ...input.grant, bindingId: '0'.repeat(64) },
    { ...input.grant, repositoryId: 'other' }, { ...input.grant, actorId: 'other' }]) {
    expect(await generateSourcePatch({ ...input, checkout, grant }, NOW)).toMatchObject({ status: 'refused', reason: 'unauthorized_generation' });
  }
  expect(await generateSourcePatch({ ...input, checkout, evidence: { ...input.evidence, digest: '0'.repeat(64) } }, NOW)).toMatchObject({ status: 'refused', reason: 'digest_mismatch' });
  expect(await generateSourcePatch({ ...input, checkout }, NOW + 8 * 86400000)).toMatchObject({ status: 'refused', reason: 'expired' });
});
it('refuses mismatched operation, side, tenant and fact selection without source reads', async () => {
  const input = await migrationFixture(roots); const checkout = { ...input.checkout, root: '/nonexistent/never-read' };
  for (const change of [{ operation: 'other' }, { side: 'response' as const }, { tenant: 'other' }]) {
    const authority = { ...input.grant.authority, ...change };
    expect(await generateSourcePatch({ ...input, checkout, grant: { ...input.grant, authority } }, NOW)).toMatchObject({ status: 'refused', reason: 'scope_mismatch' });
  }
  expect(await generateSourcePatch({ ...input, checkout, grant: { ...input.grant, factId: 'missing' } }, NOW)).toMatchObject({ status: 'refused', reason: 'missing_facts' });
});
it('rejects prototype-sensitive migrations even with a reviewed digest', async () => {
  const input = await migrationFixture(roots);
  const evidence = { ...input.evidence, facts: [{ ...input.evidence.facts[0], to: '/__proto__' }] };
  const signed = { ...evidence, digest: await sourceRepairDigest({ ...input.evidence, facts: [{ ...input.evidence.facts[0], to: '/__proto__' }] }) };
  expect(await generateSourcePatch({ ...input, evidence: signed, grant: { ...input.grant, evidenceDigest: signed.digest } }, NOW)).toMatchObject({ status: 'refused', reason: 'unsafe_field' });
});
it('refuses a claimed no-op if both source and destination keys remain in the bound serializer', async () => {
  const source = 'function serialize(input) { return { source: input.source, payment_method: input.other }; }';
  const input = await migrationFixture(roots, source, 'request', 'payment_method');
  expect(await generateSourcePatch(input, NOW)).toMatchObject({ status: 'refused', reason: 'collision' });
});
it.each([
  'function serialize(input) { return { ["source"]: input.source }; }',
  'function serialize(input) { const alias = input; return { source: alias.source }; }',
  'function serialize(input) { return { ...input, source: input.source }; }',
])('refuses an unsupported target even when submitted as a structurally valid binding: %s', async source => {
  const input = await migrationFixture(roots);
  writeFileSync(join(input.checkout.root, 'api.ts'), source);
  // Recommit the source; arbitrary submitted metadata cannot bypass AST admission.
  const { git } = await import('./support/source-migration.js');
  git(input.checkout.root, 'add', 'api.ts');
  git(input.checkout.root, '-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.test', '-c', 'commit.gpgsign=false', 'commit', '-qm', 'unsupported');
  const checkout = { ...input.checkout, baseSha: git(input.checkout.root, 'rev-parse', 'HEAD') };
  const binding = { ...input.binding, baseSha: checkout.baseSha, fileDigest: digest(source) };
  expect(await generateSourcePatch({ ...input, checkout, binding }, NOW)).toMatchObject({ status: 'refused', reason: 'unsupported_syntax' });
  expect(readFileSync(join(checkout.root, 'api.ts'), 'utf8')).toBe(source);
});
it('detaches caller inputs before asynchronous digest verification', async () => {
  const input = await migrationFixture(roots); const grant = { ...input.grant };
  const result = generateSourcePatch({ ...input, grant }, NOW);
  grant.bindingId = '0'.repeat(64);
  expect((await result).status).toBe('generated');
});
it('encodes unusual path bytes in a Git-parseable patch and preserves Unicode source context', async () => {
  const file = 'api café\t.ts'; const source = '// é😀\nfunction serialize(input) { return { source: input.source }; }';
  const input = await migrationFixture(roots, source, 'request', 'source', file);
  const result = await generateSourcePatch(input, NOW);
  expect(result.status).toBe('generated');
  if (result.status !== 'generated') throw Error('missing patch');
  execFileSync('git', ['-C', input.checkout.root, 'apply', '--check', '-'], { input: result.patch });
  execFileSync('git', ['-C', input.checkout.root, 'apply', '-'], { input: result.patch });
  expect(readFileSync(join(input.checkout.root, file), 'utf8')).toBe(source.replace('{ source:', '{ payment_method:'));
});
