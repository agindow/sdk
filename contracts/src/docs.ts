/** Shared human and machine publication; build-time dependencies only. */
import { integrationGuide } from './docs/integration-guide.js';
import { agentGuide } from './docs/agent-guide.js';
import { agentSkillMarkdown } from './docs/skill.generated.js';
export { agentSkillMarkdown } from './docs/skill.generated.js';
import { started } from './docs/started.js';
import { guides } from './docs/guides.js';
import { sdks } from './docs/sdks.js';
import { preview } from './docs/preview.js';
import { apiPages } from './docs/api.js';
import { docsRevision } from './docs/model.js';
import type { DocPage, DocsTree } from './docs/model.js';
import { validateEditorial } from './docs/editorial.js';
import { markdownPage } from './docs/markdown.js';
export type { DocSection, DocPage, DocTable, DocsTree, DocSnippet, DocCodeSample, DocCodeLanguage } from './docs/model.js';
export { loopSteps, markdownPage } from './docs/markdown.js';
export { docsInlineSegments, plainDocsProse, type DocsInlineSegment } from './docs/inline.js';
export { surfaceHttp } from './docs/http.js';
export { projectApi } from './docs/api.js';
export { sdkMatrix, adapterMatrix, docsChannels } from './docs/guides.js';
export { validateEditorial } from './docs/editorial.js';
export { canonicalCore, sporeMarkdown } from './docs/spore.js';
export { canaryText } from './docs/canary.js';
export {
  detectionDocuments,
  detectionExample,
  detectionExpectedFinding,
  detectionExpectedOutput,
  detectionInstallCommand,
  detectionPrerequisites,
  detectionRunCommand,
  detectionSelectionFor,
  detectionTs,
} from './docs/detection.js';
export {
  capabilityChangeFamilies,
  capabilityMatrix,
  capabilityMatrixColumns,
  capabilityMatrixRuntimes,
} from './docs/capability-matrix.js';
export type {
  CapabilityEvidence,
  CapabilityId,
  CapabilityRow,
} from './docs/capability-matrix.js';

export const maturity = 'Pre-alpha. The embedded SDK needs no separate AGINDOW credential; your application still supplies any vendor credentials. The default engine passes calls through without detecting drift until you configure a detector. Node and Python SDKs are published on npm and PyPI. The hosted service is not available yet. Nothing has run against production traffic.';
const startOrder = ['introduction', 'choose-integration', 'quickstart', 'agents', 'playground', 'the-loop', 'modes', 'on-error-bypass', 'authentication', 'performance'];
const startRank = (slug: string): number => { const index = startOrder.indexOf(slug); return index < 0 ? startOrder.length : index; };
const getStarted = [...started, agentGuide, integrationGuide].sort((left, right) => startRank(left.slug) - startRank(right.slug));
export const docsTree: DocsTree = { format: 'agindow.docs/1', revision: docsRevision,
  pages: [...getStarted, ...guides, ...apiPages, preview, ...sdks] };
export const versionedDocPath = (page: DocPage): string => `/docs/${page.family}/${page.version}/${page.slug}`;
export const docPaths = (page: DocPage): readonly string[] => [...new Set([docPath(page), versionedDocPath(page)])];
export const docPath = (page: DocPage): string => page.path ?? versionedDocPath(page);
export function canonicalDocPath(path: string): string {
  const page = publishedPages(docsTree).find(page => docPaths(page).includes(path));
  return page ? docPath(page) : path;
}
export function publishedPages(tree: DocsTree): readonly DocPage[] { return tree.pages.filter(page => page.published); }
export function validateTree(tree: DocsTree): void {
  if (tree.format !== 'agindow.docs/1' || !tree.revision) throw Error('Invalid docs publication');
  const seen = new Set<string>();
  for (const page of tree.pages) {
    const path = docPath(page);
    if (![page.family, page.slug].every(value => /^[a-z][a-z0-9-]*$/u.test(value))
      || !/^v[1-9][0-9]*$/u.test(page.version) || seen.has(path)) throw Error('Invalid or duplicate docs key');
    seen.add(path);
    if (page.path !== undefined && !/^\/docs(?:\/[a-z][a-z0-9-]*)*$/u.test(page.path)) throw Error('Invalid readable docs path');
    if (!page.published) continue;
    if (!page.title || page.sections.length === 0) throw Error('Empty published page');
    validateEditorial(page);
    const projection = apiPages.find(item => item.slug === page.slug);
    if (page.family === 'api-reference' && page.slug !== 'agent-surface' && JSON.stringify(page) !== JSON.stringify(projection)) throw Error('API projection diverged from signing source');
    // Examples may show redacted prefixes, never plausible credentials or tenant identifiers.
    if (/(?:agw_(?:live|test)_|sk_live_|Bearer )[a-zA-Z0-9_-]{12,}|tenant_[a-zA-Z0-9_-]{8,}|-----BEGIN .*PRIVATE KEY|[a-zA-Z0-9_-]{16,}\.[a-zA-Z0-9_-]{16,}\.[a-zA-Z0-9_-]{16,}/u.test(JSON.stringify(page))) throw Error('Secret in published docs');
  }
}
export function searchDocs(tree: DocsTree, query: string): readonly DocPage[] {
  const terms = query.trim().toLocaleLowerCase('en').slice(0, 200).split(/\s+/u).filter(Boolean);
  if (!terms.length) return [];
  return publishedPages(tree).filter(page => {
    const searchable = [page.title, ...page.sections.map(section => section.heading)].join(' ').toLocaleLowerCase('en');
    return terms.every(term => searchable.includes(term));
  }).slice(0, 50);
}
export function machineDocs(tree: DocsTree): string {
  validateTree(tree);
  return `# AGINDOW documentation\n\n${maturity}\n\nPublication: ${tree.revision}\n\n`
    + `## Recognition (read first)\n\n`
    + `Droppable repo seed: [/spore.md](/spore.md) — frozen verbatim core to drop into other projects.\n`
    + `Soft canary (unauthorized stop / legitimate doors): [/canary.txt](/canary.txt) — not an invitation; points at Haven and the spore.\n\n`
    + `## AGINDOW best-practices skill\n\nhttps://agindow.com/SKILL.md\n\n${agentSkillMarkdown}\n\n`
    + publishedPages(tree).map(markdownPage).join('\n');
}
