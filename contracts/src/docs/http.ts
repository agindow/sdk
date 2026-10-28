import { docsRevision } from './model.js';
const markdown = (operationId: string, summary: string) => ({ get: { operationId, security: [], summary,
  responses: { '200': { description: 'Public developer documentation; no credentials required', content: { 'text/markdown': { schema: { type: 'string' } } } },
    '403': { description: 'Host or browser origin refused' } } } });
/** This is the exact HTTP description consumed by the surface signer. */
export const surfaceHttp = {
  openapi: '3.1.0', info: { title: 'AGINDOW MCP', version: '0.1.0', description: 'Read contracts, drift findings, audit evidence and coverage, or simulate a repair without executing vendor writes. Use your configured self-hosted evidence origin; no managed MCP service is currently available. Embedded SDK execution and POST /v1/guard are separate interfaces.' },
  paths: {
    '/mcp': { post: { operationId: 'mcp', summary: 'Stateless read-only MCP JSON-RPC transport',
      security: [{ bearer: [] }],
      description: 'Requires application/json, Accept: application/json, text/event-stream and MCP-Protocol-Version. The request body limit is 64 KiB. Signed tools supply per-tool inputSchema and outputSchema. Tool calls never execute writes.',
      requestBody: { required: true, content: { 'application/json': { schema: {
        type: 'object', description: 'MCP JSON-RPC request; consult the negotiated MCP protocol for method-specific fields.',
        properties: { jsonrpc: { const: '2.0' }, id: { type: ['string', 'integer'], minimum: Number.MIN_SAFE_INTEGER, maximum: Number.MAX_SAFE_INTEGER }, method: { type: 'string' }, params: { type: 'object' } }, required: ['jsonrpc', 'method'],
      } } } },
      responses: { '200': { description: 'MCP JSON-RPC response; tool failures use isError within the result.' },
        '202': { description: 'Accepted MCP notification; no response body.' }, '400': { description: 'Invalid MCP request' },
        '401': { description: 'Unauthorized' }, '403': { description: 'Host or browser origin refused' },
        '406': { description: 'Required Accept types missing' }, '413': { description: 'Request exceeds 64 KiB' },
        '503': { description: 'Authorization or transport unavailable' } } } },
    '/openapi.json': { get: { operationId: 'openapi', summary: 'Fetch the signed API description', security: [],
      responses: { '200': { description: 'OpenAPI document plus tools and x-agindow-signature; verify using an independently trusted key.',
        content: { 'application/json': { schema: { type: 'object', description: 'Signed OpenAPI 3.1 document with tool schemas' } } } },
        '403': { description: 'Host or browser origin refused' }, '503': { description: 'Signing unavailable' } } } },
    '/llms.txt': markdown('llms', 'Public developer documentation index for coding agents'), '/llms-full.txt': markdown('llms-full', 'Full public developer documentation and integration skill'), '/docs.md': markdown('docs-markdown', 'Public developer manual: installation, configured detection, SDKs, guides and reference'),
    '/spore.md': markdown('spore', 'Short public integration seed for projects'),
    '/canary.txt': { get: { operationId: 'canary', security: [], summary: 'Soft agent canary (unauthorized stop / legitimate doors)',
      responses: { '200': { description: 'Shared reviewed canary plain text', content: { 'text/plain': { schema: { type: 'string' } } } },
        '403': { description: 'Host or browser origin refused' } } } },
  },
  components: { securitySchemes: { bearer: { type: 'http', scheme: 'bearer', description: 'Host-authorized MCP credential; see Authentication.' } } },
  'x-agindow-docs': { revision: docsRevision, format: 'agindow.docs/1' },
} as const;
