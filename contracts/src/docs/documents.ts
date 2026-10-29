import { sdkReleaseNotice } from './release-status.js';
import { docsIntroduction, productMaturity } from './positioning.js';
/** Public document formats; repository publishing instructions never enter these bodies. */
import type { DocsTree } from './model.js';
import { markdownPage } from './markdown.js';
import { sdkMatrix } from './sdk-matrix.generated.js';

const path = (page: DocsTree['pages'][number]): string => page.path ?? `/docs/${page.family}/${page.version}/${page.slug}`;
const availablePages = (tree: DocsTree) => tree.pages.filter(page => page.published);
const introduction = `${sdkReleaseNotice}\n\n${docsIntroduction}

${productMaturity}
The published pre-alpha Node and Python SDKs run an embedded statistical guest.
Configure explicit scoped baseline/live windows, start in observe mode, and verify
findings separately from bypass events. Selective repair is off by default; enable reviewed rename rules with independent verification. AGINDOW does not currently operate a hosted guard or MCP service.

Install the package for your application runtime:

- Node 22+: \`npm install agindow@${sdkMatrix.node.version}\`
- Python 3.11+: \`pip install agindow==${sdkMatrix.python.version}\`

The embedded SDK needs no separate AGINDOW credential. Keep vendor credentials
in your existing client. A successful installation does not configure detection;
a synthetic finding does not establish production accuracy or write correctness.
`;
const resources = `## Public document formats

- [Developer documentation](https://agindow.com/docs): browser reading and search.
- [Documentation index](https://agindow.com/llms.txt): page links for coding agents.
- [Full agent documentation](https://agindow.com/llms-full.txt): public manual plus the integration skill.
- [Developer manual](https://agindow.com/docs.md): public documentation as Markdown.
- [Integration skill](https://agindow.com/SKILL.md): reusable coding-agent guidance; review before enabling.
- [Integration spore](https://agindow.com/spore.md): optional short project seed.
- [OpenAPI](https://agindow.com/openapi.json): read-only MCP description; verify its signature using an independently trusted key.
- [Soft canary](https://agindow.com/canary.txt): optional agent-facing stop and discovery note.

SKILL.md, spore.md and canary.txt are served by the public website. The other
machine-document mirrors require an enabled evidence service and may return 503
on the public website. For a self-hosted MCP deployment, use the same paths on
its configured origin. Never infer an execution endpoint from these document links.
`;

export function publicDocumentation(tree: DocsTree): string {
  return `# AGINDOW developer documentation\n\n${introduction}\n`
    + `Start with [Quickstart](/docs/quickstart), then choose [Node](/docs/sdk/node) or [Python](/docs/sdk/python).\n\n`
    + availablePages(tree).map(markdownPage).join('\n') + `\n${resources}`;
}
export function documentationIndex(tree: DocsTree): string {
  return `# AGINDOW\n\n${introduction}\n## Documentation pages\n\n`
    + availablePages(tree).map(page => `- [${page.title}](https://agindow.com${path(page)}): ${page.sections[0]?.text.split('. ')[0] ?? 'Public developer reference'}.`).join('\n')
    + `\n\n${resources}`;
}
