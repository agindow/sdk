import { runtimePages } from './runtime.js';
import { machineApiPages } from './machine-api.js';
import { operationSections } from './operation.js';
import type { DocPage, DocSection } from './model.js';
import { contract } from './model.js';
import { surfaceHttp } from './http.js';
const links: DocSection = { heading: 'Authentication and errors', text: 'Credentials and runtime policy have distinct owners. The surface uses MCP transport errors; the guard runtime taxonomy is separate.', links: [{ label: 'Authentication', href: '/docs/authentication' }, { label: 'Runtime error taxonomy', href: '/docs/guides/errors' }] };
export const signature: DocSection = { heading: 'Verify Ed25519 before use', text: 'Fetch GET /openapi.json from your configured surface origin. Read x-agindow-signature, require algorithm Ed25519 and canonicalization agindow.canonical-json/1. Resolve key_id through an independently trusted public-key configuration. Remove x-agindow-signature from the document; recursively sort object keys, preserve array order and JSON primitives, and encode the canonical JSON as UTF-8. Decode the base64 signature and verify the exact bytes with the pinned Ed25519 key. Reject unknown key_id, algorithm or canonicalization and any verification failure. Never trust a key delivered beside the document. The signed tools inputSchema/outputSchema are included in those bytes.', links: [{ label: 'API reference', href: '/docs/api' }] };
interface HttpDescription { readonly paths: Readonly<Record<string, Readonly<Record<string, unknown>>>> }
const page = (slug: string, title: string, sections: readonly DocSection[]): DocPage => ({ family: 'api-reference', version: 'v1', slug, path: slug === 'index' ? '/docs/api' : `/docs/api/${slug}`, title, published: true, ...(slug === 'index' ? { navTitle: 'Overview' } : slug === 'mcp' ? { navTitle: 'HTTP transport' } : {}), sections: slug === 'index' ? sections : [...sections, links] });
/** Entire operation shape is projected, including nested fields; prose owns no schema. */
export function projectApi(document: HttpDescription): readonly DocPage[] {
  const operations = Object.entries(document.paths).flatMap(([path, methods]) => Object.entries(methods)
    .filter(([method]) => ['get', 'post', 'put', 'patch', 'delete', 'head', 'options', 'trace'].includes(method))
    .map(([method, operation]) => ({ path, method, operation, slug: path === '/openapi.json' ? 'openapi' : path === '/mcp' ? 'mcp' : `${method}-${path.slice(1).replace(/[^a-z0-9]+/gu, '-')}` })));
  return [
    page('index', 'API drift documentation — API index', [
      { heading: 'Choose your integration', aliases: ['choose-the-interface-for-your-application'], text: 'Most application developers start with the embedded Node or Python SDK. Use the reference groups below when you choose HTTP execution, workload authentication, read-only MCP evidence or machine-readable documentation.', links: [{ label: 'Integration chooser', href: '/docs/choose-integration' }, { label: 'Node SDK', href: '/docs/sdk/node' }, { label: 'Python SDK', href: '/docs/sdk/python' }, { label: 'Node method reference', href: '/docs/sdk/node-reference' }, { label: 'Python method reference', href: '/docs/sdk/python-reference' }] },
      { heading: 'Guard execution', aliases: ['separate-execution-host'], text: 'Execute a tool call with POST /v1/guard on your own host. Read the envelope fields, upstream response behavior, diagnostic headers, errors and safe recovery steps. Audit and retained-outcome reads belong to this execution interface.', links: [{ label: 'Execute a tool call', href: '/docs/api/guard' }, { label: 'Audit artifact schema', href: '/docs/api/audit-artifact' }, { label: 'Health, audit and outcomes', href: '/docs/api/recovery' }, { label: 'Remote HTTP client', href: '/docs/sdk/remote' }] },
      { heading: 'Workload identity (optional)', aliases: ['authentication-and-errors'], text: 'Exchange a workload identity with POST /v1/token when your execution host requires an admitted, sender-bound identity. This endpoint issues a scoped token; it does not execute tools, create an account or authenticate a dashboard browser session. Embedded SDK use does not require this exchange.', links: [{ label: 'Exchange a workload identity', href: '/docs/api/token' }, { label: 'Credentials and authentication', href: '/docs/authentication' }] },
      { heading: 'MCP evidence (read-only)', aliases: ['mcp-transport-and-document-routes'], text: 'Read evidence through MCP: contracts, drift history, audit records, coverage and simulated repair proposals. The tools never execute vendor writes. Read the tool inputs/results, then the transport details or runnable MCP example.', links: [{ label: 'Tool inputs and results', href: '/docs/api/tools' }, { label: 'HTTP transport', href: '/docs/api/mcp' }, { label: 'MCP quickstart', href: '/docs/api-reference/v1/agent-surface' }] },
      { heading: 'Machine documents', text: 'Use the document endpoints for signed API descriptions, Markdown docs and agent guidance. SKILL.md is the public best-practices skill. Availability of other machine documents depends on your configured MCP service; no managed MCP origin is issued.', table: { columns: ['Method', 'Path'], rows: [...operations.filter(item => item.method === 'get').map(item => [item.method.toUpperCase(), item.path]), ['GET', '/SKILL.md']] }, links: [...operations.filter(item => item.method === 'get').map(item => ({ label: `GET ${item.path}`, href: `/docs/api/${item.slug}` })), { label: 'GET /SKILL.md', href: '/docs/api/skill' }] },
    ]),
    ...runtimePages,
    ...machineApiPages,
    ...operations.map(item => page(item.slug, item.path === '/mcp' ? 'MCP HTTP transport — POST /mcp' : `${item.method.toUpperCase()} ${item.path}`, [
      ...operationSections(item.operation),
      ...(item.path === '/openapi.json' ? [signature] : []),
    ])),
    page('skill', 'GET /SKILL.md', [
      { heading: 'AGINDOW best-practices skill', text: 'Read or download the reusable skill for your coding agent. It covers SDK setup, explicit detector configuration, observe-first evaluation and safe handling of writes and outcomes. This public document does not require an API key or an MCP connection.', links: [{ label: 'Open SKILL.md', href: '/SKILL.md' }, { label: 'Installation and usage', href: '/docs/agents' }] },
      { heading: 'Request and response', text: 'GET https://agindow.com/SKILL.md returns the complete skill with YAML frontmatter as text/markdown; charset=utf-8. HEAD returns the same response headers with no body. There are no request fields or credentials. Other methods return HTTP 405 with Allow: GET, HEAD. Saving or copying the file does not install it automatically or enable detection.', example: { language: 'bash', code: 'curl --fail --show-error --location https://agindow.com/SKILL.md' } },
    ]),
  ];
}
export const apiPages = projectApi(surfaceHttp);
