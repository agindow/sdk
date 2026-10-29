import { createAgindow } from 'agindow';

const sdk = createAgindow({ mode: 'observe' });
await sdk.ready;
sdk.on('drift', event => console.log('Drift:', event.verdict));

const getOrder = sdk.guard(async () => {
  const url = process.env.API_URL;
  if (!url) throw new Error('Set API_URL to your order JSON endpoint.');
  const response = await fetch(url, { signal: AbortSignal.timeout(10_000) });
  if (!response.ok) throw new Error(`API returned ${response.status}`);
  const order = await response.json();
  return order;
}, { tool: 'orders.get', operation: 'read' });

const order = await getOrder();
console.log('API result:', order);
console.log('AGINDOW events:', sdk.recent().map(event => event.type));
