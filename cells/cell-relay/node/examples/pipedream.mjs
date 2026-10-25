import { PipedreamClient } from '@pipedream/sdk';
import { createAgindow } from '../dist/esm/index.js';
const sdk = createAgindow(); await sdk.ready;
// Offline transport fixture. This token is synthetic and never leaves the process.
const originalFetch = globalThis.fetch;
globalThis.fetch = async () => new Response('{"ok":true}', { headers: { 'content-type': 'application/json' } });
try {
  const client = new PipedreamClient({ projectId: 'fixture', tokenProvider: { getToken: async () => 'offline-fixture' } });
  const proxy = sdk.clientAdapter(client.proxy, { tool: 'slack.chat', operation: 'write' });
  console.log(await proxy.post({ externalUserId: 'fixture', accountId: 'fixture', url: 'https://slack.com/api/chat.postMessage', body: { text: 'fixture' } }));
} finally { globalThis.fetch = originalFetch; }
