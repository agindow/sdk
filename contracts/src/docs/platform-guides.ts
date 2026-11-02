import type { DocPage } from './model.js';
import { sdkMatrix } from './sdk-matrix.generated.js';

const expectLinks = [
  { label: 'Start in observe mode', href: '/docs/guides/observe-first' },
  { label: 'Configure detection windows', href: '/docs/sdk/configuration' },
  { label: 'Detection quickstart', href: '/docs/detection' },
] as const;

const expectText = 'Observe mode preserves original payloads, results and upstream exceptions. Installing and wrapping alone does not configure detection: the statistical guest is default; configure windows to detect. An empty `recent()` history is not a clean verdict. Selective repair stays off by default and needs reviewed rename rules plus independent verification. Python remains callable-only for this platform.';

/** Dedicated per-platform how-tos under Integrations (TII-201). */
export const platformGuides: readonly DocPage[] = [
  {
    family: 'sdks',
    version: 'v1',
    slug: 'pipedream',
    path: '/docs/integrations/pipedream',
    title: 'Wrap Pipedream client.proxy',
    navTitle: 'Pipedream',
    published: true,
    sections: [
      {
        heading: 'When to use this',
        aliases: ['when-to-use'],
        text: 'Use this guide when your agent already reaches a vendor through Pipedream `client.proxy` and you want the embedded Node SDK to inspect that HTTP boundary. Prefer it after the quickstart has wrapped one owned API read. For transport choice (embedded SDK vs HTTP host vs MCP vs playground), use the chooser instead.',
        links: [
          { label: 'Third-party integrations overview', href: '/docs/integrations' },
          { label: 'Choose transport', href: '/docs/choose-integration' },
          { label: 'Quickstart', href: '/docs/quickstart' },
        ],
      },
      {
        heading: 'How to wrap client.proxy',
        aliases: ['how-to', 'wrap'],
        text: `Illustrative offline fixture for Node 22+ with published \`agindow@${sdkMatrix.node.version}\`. Wrap \`client.proxy\` with \`clientAdapter\`. The embedded SDK needs no separate AGINDOW API key; keep vendor credentials in the Pipedream client. The sample replaces \`fetch\` with a local response so it never leaves the process. Wrapping a container does not discover nested clients.`,
        example: {
          language: 'ts',
          code: `import { PipedreamClient } from '@pipedream/sdk';
import { createAgindow } from 'agindow';
const sdk = createAgindow();
await sdk.ready;
// Offline transport fixture. This token is synthetic and never leaves the process.
const originalFetch = globalThis.fetch;
globalThis.fetch = async () => new Response('{"ok":true}', { headers: { 'content-type': 'application/json' } });
try {
  const client = new PipedreamClient({ projectId: 'fixture', tokenProvider: { getToken: async () => 'offline-fixture' } });
  const proxy = sdk.clientAdapter(client.proxy, { tool: 'slack.chat', operation: 'write' });
  console.log(await proxy.post({ externalUserId: 'fixture', accountId: 'fixture', url: 'https://slack.com/api/chat.postMessage', body: { text: 'fixture' } }));
} finally {
  globalThis.fetch = originalFetch;
}`,
        },
        links: [
          { label: 'Node SDK guide', href: '/docs/sdk/node' },
          { label: 'Pipedream surface limits', href: '/integrations/pipedream' },
        ],
      },
      {
        heading: 'What to expect',
        aliases: ['what-to-expect', 'expectations'],
        text: expectText,
        links: [...expectLinks],
      },
      {
        heading: 'Reference',
        aliases: ['reference'],
        text: 'Use the Node guide for full call wrapping and configuration for detector selection. The public Pipedream page summarizes surface limits without replacing this how-to.',
        links: [
          { label: 'Node / TypeScript', href: '/docs/sdk/node' },
          { label: 'Configuration', href: '/docs/sdk/configuration' },
          { label: 'Framework matrix', href: '/docs/guides/frameworks' },
          { label: 'Other platforms', href: '/docs/integrations' },
        ],
      },
    ],
  },
  {
    family: 'sdks',
    version: 'v1',
    slug: 'langchain',
    path: '/docs/integrations/langchain',
    title: 'Wrap LangChain tools',
    navTitle: 'LangChain',
    published: true,
    sections: [
      {
        heading: 'When to use this',
        aliases: ['when-to-use'],
        text: 'Use this guide when your agent already exposes LangChain tools and you want `toolAdapter` to preserve name, description and schema while guarding the invoke / ToolCall executor. Prefer it after the quickstart. Batch and stream execution have not been qualified.',
        links: [
          { label: 'Third-party integrations overview', href: '/docs/integrations' },
          { label: 'Choose transport', href: '/docs/choose-integration' },
          { label: 'Quickstart', href: '/docs/quickstart' },
        ],
      },
      {
        heading: 'How to wrap tools',
        aliases: ['how-to', 'wrap'],
        text: `Illustrative offline tool. Install \`agindow@${sdkMatrix.node.version}\`, await \`ready\`, then wrap the tool collection you already own. \`toolAdapter\` preserves name, description and schema and guards the invoke / ToolCall executor.`,
        example: {
          language: 'ts',
          code: `import { DynamicStructuredTool } from '@langchain/core/tools';
import { createAgindow } from 'agindow';
const sdk = createAgindow();
await sdk.ready;
const existing = new DynamicStructuredTool({
  name: 'search',
  description: 'Offline search fixture',
  schema: { type: 'object', properties: { query: { type: 'string' } }, required: ['query'] },
  func: async () => 'fixture result',
});
const tools = sdk.toolAdapter([existing]);
const search = tools[0];
if (search === undefined) throw Error('missing tool');
console.log(await search.invoke({ type: 'tool_call', id: 'call_fixture', name: 'search', args: { query: 'fixture' } }));`,
        },
        links: [
          { label: 'Node adapters and signatures', href: '/docs/sdk/node-reference' },
          { label: 'LangChain surface limits', href: '/integrations/langchain' },
        ],
      },
      {
        heading: 'What to expect',
        aliases: ['what-to-expect', 'expectations'],
        text: expectText,
        links: [...expectLinks],
      },
      {
        heading: 'Reference',
        aliases: ['reference'],
        text: 'Use the Node reference for adapter signatures and the matrix for other framework shapes. The public LangChain page summarizes surface limits without replacing this how-to.',
        links: [
          { label: 'Node adapters and signatures', href: '/docs/sdk/node-reference' },
          { label: 'Configuration', href: '/docs/sdk/configuration' },
          { label: 'Framework matrix', href: '/docs/guides/frameworks' },
          { label: 'Other platforms', href: '/docs/integrations' },
        ],
      },
    ],
  },
  {
    family: 'sdks',
    version: 'v1',
    slug: 'nango',
    path: '/docs/integrations/nango',
    title: 'Wrap Nango proxy and sandbox codegen',
    navTitle: 'Nango',
    published: true,
    sections: [
      {
        heading: 'When to use this',
        aliases: ['when-to-use'],
        text: 'Use this guide when your Node runtime can install npm and you want Surface A `clientAdapter` around Nango HTTP proxy methods, or when a Nango sandbox forbids npm and you need the CLI report/codegen path instead. Auth and storage helpers such as `batchSave` and checkpoints never enter drift evaluation.',
        links: [
          { label: 'Third-party integrations overview', href: '/docs/integrations' },
          { label: 'CLI report and sandbox codegen', href: '/docs/sdk/cli' },
          { label: 'Quickstart', href: '/docs/quickstart' },
        ],
      },
      {
        heading: 'How to wrap the proxy',
        aliases: ['how-to', 'wrap'],
        text: `Illustrative excerpt for Node 22+ with published \`agindow@${sdkMatrix.node.version}\`. Surface A wraps HTTP proxy methods with \`clientAdapter\` where npm is allowed. \`triggerAction\` is not an HTTP allow-list method; guard it explicitly if you need action output. For sandboxes that forbid npm, use the CLI report and limited codegen path instead of installing WASM inside the sandbox. Generated code has not been tested in a deployed Nango sandbox.`,
        example: {
          language: 'ts',
          code: `import { Nango } from '@nangohq/node';
import { createAgindow } from 'agindow';
const sdk = createAgindow({ mode: 'observe' });
await sdk.ready;
const client = new Nango({ secretKey: process.env.NANGO_SECRET_KEY ?? '' });
const proxy = sdk.clientAdapter(client, {
  tool: (request: { endpoint: string }) => \`nango/\${request.endpoint}\`,
});`,
        },
        links: [
          { label: 'CLI report and sandbox codegen', href: '/docs/sdk/cli' },
          { label: 'Nango surface limits', href: '/integrations/nango' },
        ],
      },
      {
        heading: 'What to expect',
        aliases: ['what-to-expect', 'expectations'],
        text: expectText,
        links: [...expectLinks],
      },
      {
        heading: 'Reference',
        aliases: ['reference'],
        text: 'Use the CLI guide for no-npm sandboxes and the Node guide when npm is allowed. The public Nango page summarizes surface limits without replacing this how-to.',
        links: [
          { label: 'CLI and offline reports', href: '/docs/sdk/cli' },
          { label: 'Node / TypeScript', href: '/docs/sdk/node' },
          { label: 'Framework matrix', href: '/docs/guides/frameworks' },
          { label: 'Other platforms', href: '/docs/integrations' },
        ],
      },
    ],
  },
  {
    family: 'sdks',
    version: 'v1',
    slug: 'mastra',
    path: '/docs/integrations/mastra',
    title: 'Wrap Mastra execute-shaped tools',
    navTitle: 'Mastra',
    published: true,
    sections: [
      {
        heading: 'When to use this',
        aliases: ['when-to-use'],
        text: 'Use this guide when Mastra exposes a supported execute-style tool map and you want the same Node `toolAdapter` path used for other execute shapes. Prefer the adapter matrix and Node guide over inventing a complete Mastra sample. Full framework middleware, batch/stream and end-to-end Mastra execution are not qualified.',
        links: [
          { label: 'Third-party integrations overview', href: '/docs/integrations' },
          { label: 'Framework adapter matrix', href: '/docs/guides/frameworks' },
          { label: 'Node integration', href: '/docs/sdk/node' },
        ],
      },
      {
        heading: 'How to wrap execute shapes',
        aliases: ['how-to', 'wrap'],
        text: 'Where Mastra exposes a supported execute-style tool map, use Node `toolAdapter` as for other execute shapes. Current evidence is a local shape fixture: id, description, inputSchema identity and execution context can be retained when the shape matches. No fabricated end-to-end Mastra sample is published here.',
        links: [
          { label: 'Framework adapter matrix', href: '/docs/guides/frameworks' },
          { label: 'Node integration', href: '/docs/sdk/node' },
          { label: 'Mastra surface limits', href: '/integrations/mastra' },
        ],
      },
      {
        heading: 'What to expect',
        aliases: ['what-to-expect', 'expectations'],
        text: expectText,
        links: [...expectLinks],
      },
      {
        heading: 'Reference',
        aliases: ['reference'],
        text: 'Confirm the matrix row before wrapping. The public Mastra page summarizes surface limits without replacing this how-to.',
        links: [
          { label: 'Framework matrix', href: '/docs/guides/frameworks' },
          { label: 'Node / TypeScript', href: '/docs/sdk/node' },
          { label: 'Configuration', href: '/docs/sdk/configuration' },
          { label: 'Other platforms', href: '/docs/integrations' },
        ],
      },
    ],
  },
];
