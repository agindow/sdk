import { mkdtempSync, realpathSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { parseSourceRepairEvidence, sourceRepairDigest } from 'contracts';
import type { SourceRepairAuthority, SourceRepairEvidence } from 'contracts';
import { bindSource } from '../../cli/binding/index.js';
import type { BindingSelection, CheckoutGrant } from '../../cli/binding/types.js';
import type { MigrationInput } from '../../cli/migration/types.js';
export const NOW = Date.parse('2026-10-20T12:00:01Z');
export const requestSource = `import { client as aliased } from 'vendor-not-installed';\r\n// local model source stays local\r\nexport function serialize(input: { source: string }) {\r\n  return { /* wire */ source /* keep */: input.source };\r\n}\r\nconst unrelated = { source: 'same name' };\r\nthrow Error('source must never execute');`;
export function git(root: string, ...args: string[]): string {
  return execFileSync('git', ['-C', root, '-c', 'core.hooksPath=/dev/null', ...args], { encoding: 'utf8' }).trim();
}
export async function migrationFixture(roots: string[], source = requestSource, side: 'request' | 'response' = 'request', field = 'source', file = 'api.ts'): Promise<MigrationInput> {
  const root = realpathSync(mkdtempSync(join(tmpdir(), 'agw-migration-'))); roots.push(root);
  writeFileSync(join(root, file), source);
  git(root, 'init', '-q'); git(root, 'add', file);
  git(root, '-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.test', '-c', 'commit.gpgsign=false', 'commit', '-qm', 'fixture');
  const checkout: CheckoutGrant = { root, tenantId: 'acme', repositoryId: 'repo-a', baseSha: git(root, 'rev-parse', 'HEAD'), actorId: 'owner', authorityRevision: 'checkout-1' };
  const selection: BindingSelection = { tenantId: 'acme', repositoryId: 'repo-a', operation: 'write', side,
    file, symbol: side === 'request' ? 'serialize' : 'consume', field, bindingVersion: 1 };
  const binding = bindSource(checkout, selection);
  if (binding.status !== 'bound') throw Error(`fixture binding failed: ${binding.reason}`);
  const raw = parseSourceRepairEvidence(JSON.parse(readFileSync(new URL('../../../../../contracts/examples/source_repair_evidence.valid.json', import.meta.url), 'utf8')));
  const scope = { ...raw.scope, side };
  const evidence: SourceRepairEvidence = { ...raw, scope, facts: [{ ...raw.facts[0], scope }] };
  evidence.digest = await sourceRepairDigest(evidence);
  const authority: SourceRepairAuthority = { ...scope, finding_id: evidence.finding.id, finding_revision: evidence.finding.revision };
  return { checkout, binding: binding.manifest, evidence, grant: { authority, evidenceDigest: evidence.digest, bindingId: binding.manifest.bindingId,
    factId: evidence.facts[0].id, repositoryId: 'repo-a', actorId: 'owner', authorityRevision: 'generation-1' } };
}
