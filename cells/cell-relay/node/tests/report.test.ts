import { afterEach, expect, it, vi } from 'vitest';
import { mkdtempSync, readFileSync, rmSync, writeFileSync, readdirSync, mkdirSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createHash } from 'node:crypto';
import { report } from '../cli/report/index.js';
import { scanSources } from '../cli/report/scan.js';
const directories: string[] = [];
afterEach(() => { for (const directory of directories.splice(0)) rmSync(directory, { force: true, recursive: true }); vi.restoreAllMocks(); });
const toolsSource = `import Stripe from 'stripe';
import { agindow } from 'agindow';
const stripe = new Stripe('unused');
const tools = { create: { name: 'stripe.create_customer', description: 'create', execute: async (input: object) => stripe.customers.create(input) } };
const guarded = agindow.toolAdapter(tools);
const otherTools = { list: { name: 'shopify.list_orders', execute: () => fetch('https://shopify.com/orders') } };
throw Error('source must never execute');`;
function fixture(): string {
  const directory = mkdtempSync(join(tmpdir(), 'agw-report-')); directories.push(directory);
  writeFileSync(join(directory, 'tools.ts'), toolsSource);
  writeFileSync(join(directory, 'broken.ts'), 'export const = {');
  writeFileSync(join(directory, 'comment.js'), '// const fake = { execute: () => 3 };\nconst text = "execute: () => stripe.create()";');
  return directory;
}
function hash(directory: string): string {
  return createHash('sha256').update(readdirSync(directory).sort().map(name => `${name}:${readFileSync(join(directory, name)).toString('hex')}`).join('\n')).digest('hex');
}
it('finds static tools, vendor calls and wrapper references without execution, writes or network', () => {
  const directory = fixture(); const before = hash(directory);
  const fetch = vi.spyOn(globalThis, 'fetch').mockImplementation(() => { throw Error('network forbidden'); });
  const result = report(directory);
  expect(result.tools.map(tool => [tool.name, tool.wrapped, tool.vendors])).toEqual([
    ['stripe.create_customer', true, ['stripe']], ['shopify.list_orders', false, ['shopify']],
  ]);
  expect(result.skipped).toBe(1); expect(result.text).toContain('Already wrapped: 1');
  expect(result.text).toContain('No registry snapshot'); expect(result.text).toContain('evidence unavailable');
  expect(fetch).not.toHaveBeenCalled(); expect(hash(directory)).toBe(before);
});
it('validates local event envelopes and displays the exact evidence window, changes and semantic BLOCK class', () => {
  const directory = fixture();
  const event: unknown = JSON.parse(readFileSync(new URL('../../../../contracts/examples/drift_event.valid.json', import.meta.url), 'utf8'));
  const snapshot = join(directory, 'snapshot.json');
  writeFileSync(snapshot, JSON.stringify({ version: 1, window: { start: '2026-10-01T00:00:00Z', end: '2026-10-20T00:00:00Z' }, events: [
    event, { ...(event as object), id: 'drift_shopify_fixture', tool: 'shopify.list_orders', drift: { class: 'semantic', side: 'response', vendor: 'shopify',
      changes: [{ path: 'status', kind: 'enum_expanded', from: 'old', to: 'new' }], first_seen: '2026-10-17T13:41:02Z', occurrences: 1 }, decision: 'blocked' },
  ] }));
  const result = report(directory, { snapshot });
  expect(result.text).toContain('source → payment_method'); expect(result.text).toContain('confidence 0.97');
  expect(result.text).toContain('BLOCK'); expect(result.text).toContain('2026-10-01');
  writeFileSync(snapshot, '{"version":1,"events":[{"decision":"repaired"}]}');
  expect(report(directory, { snapshot }).text).toContain('Invalid registry snapshot');
});
it('bounds files and skips symlinks instead of leaving the selected project', () => {
  const directory = fixture();
  expect(report(directory, { maxBytes: 8 }).skipped).toBeGreaterThan(0);
  expect(report(directory, { maxFiles: 1 }).skipped).toBeGreaterThan(0);
});
it('detects function maps and imported LangChain factories through local wrapper references', () => {
  const directory = mkdtempSync(join(tmpdir(), 'agw-report-map-')); directories.push(directory);
  writeFileSync(join(directory, 'tools.ts'), `import { tool } from '@langchain/core/tools';
import { agindow } from 'agindow';
const lookup = tool(async () => 'ok', { name: 'vendor.lookup', description: 'lookup' });
const tools = { plain: async () => fetch('https://api.stripe.com/v1/customers'), lookup };
agindow.toolAdapter(tools);`);
  const result = report(directory);
  expect(result.tools.map(item => item.name).sort()).toEqual(['plain', 'vendor.lookup']);
  expect(result.tools.every(item => item.wrapped)).toBe(true); expect(result.vendors).toEqual(['stripe']);
});
it('reports actual offline repair/block/clean verdicts only with explicit contracts and recordings', () => {
  const directory = fixture();
  const generation: { contracts: Record<string, unknown>[] } = JSON.parse(readFileSync(new URL('../examples/nango-contracts.json', import.meta.url), 'utf8'));
  const contract = generation.contracts[0]; if (contract === undefined) throw Error('missing fixture');
  const snapshot = join(directory, 'snapshot.json');
  writeFileSync(snapshot, JSON.stringify({ version: 1, window: { start: '2026-10-01T00:00:00Z', end: '2026-10-20T00:00:00Z' }, events: [], recordings: [
    { tool: 'stripe.create_customer', contract, args: { source: 'fictional' }, recorded: { status: 'active' } },
    { tool: 'shopify.list_orders', contract: { ...contract, tool: 'shopify.list_orders' }, args: { payment_method: 'fictional' }, recorded: { status: 'new' } },
  ] }));
  const output = report(directory, { snapshot }).text;
  expect(output).toContain('would repair 1 change'); expect(output).toContain('confidence 0.99');
  expect(output).toContain('BLOCK-class warning');
});

it('renders a sorted distinct vendors summary even when several tools share a vendor', () => {
  const directory = fixture();
  writeFileSync(join(directory, 'more.ts'), `const tools = { another: { name: 'stripe.lookup', execute: () => fetch('https://api.stripe.com/v1/customers') } };`);
  const result = report(directory);
  expect(result.tools).toHaveLength(3);
  expect(result.vendors).toEqual(['shopify', 'stripe']);
  expect(result.text.split('\n')).toContain('Vendors: 2 (shopify, stripe)');
});
it('discovers imported Nango action and sync factories with path-qualified source identities', () => {
  const directory = fixture();
  mkdirSync(join(directory, 'integrations/stripe/actions'), { recursive: true });
  mkdirSync(join(directory, 'integrations/slack/syncs'), { recursive: true });
  writeFileSync(join(directory, 'integrations/stripe/actions/get-price.ts'), `import { createAction as action } from 'nango';
const definition = action({ exec: async (nango, input) => nango.get({endpoint: '/v1/prices'}) }); export default definition;`);
  writeFileSync(join(directory, 'integrations/slack/syncs/users.ts'), `import { createSync } from 'nango';
export default createSync({exec: async (nango) => { await nango.get({endpoint:'/users.list'}); }});`);
  writeFileSync(join(directory, 'unrelated.ts'), `const createAction = (value: unknown) => value; createAction({exec: async () => 1});`);
  const tools = report(directory).tools.filter(tool => tool.name.startsWith('nango/'));
  expect(tools.map(tool => [tool.name, tool.wrapped, tool.vendors])).toEqual([
    ['nango/integrations/slack/syncs/users', false, ['slack']],
    ['nango/integrations/stripe/actions/get-price', false, ['stripe']],
  ]);
});
it('matches directory scan tool inventory from in-memory scanSources without fs', () => {
  const directory = fixture();
  const fromDisk = report(directory);
  const fromMemory = scanSources([
    { name: 'tools.ts', text: toolsSource },
    { name: 'broken.ts', text: 'export const = {' },
    { name: 'comment.js', text: '// const fake = { execute: () => 3 };\nconst text = "execute: () => stripe.create()";' },
    { name: 'notes.md', text: '# not scanned' },
  ]);
  expect(fromMemory.tools.map(tool => [tool.name, tool.wrapped, tool.vendors]))
    .toEqual(fromDisk.tools.map(tool => [tool.name, tool.wrapped, tool.vendors]));
  expect(fromMemory.skipped).toBe(2); // broken.ts + notes.md
});
