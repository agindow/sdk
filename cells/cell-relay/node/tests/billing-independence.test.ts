import { createRequire } from 'node:module';
import { afterEach, expect, it, vi } from 'vitest';
import { createAgindow } from '../src/index.js';
import { bindGuest } from '../src/wasm.js';
afterEach(() => vi.unstubAllGlobals());
it('TII-42 keeps actual WASM detection and passthrough independent of unreachable billing', async () => {
  const network = vi.fn(async () => {
    throw Error('Billing unavailable or allowance exhausted');
  });
  vi.stubGlobal('fetch', network);
  const sdk = createAgindow(
    {
      mode: 'strict',
      selection: {
        tenant: 'synthetic',
        connection: 'synthetic',
        vendor: 'synthetic',
        operation: 'op',
        segment: 's',
        fields: [
          {
            side: 'response',
            path: '/value',
            baseline: Array<string>(128).fill('{"value":1}'),
            live: Array<string>(128).fill('{"value":100}'),
            enumeration: true,
          },
        ],
      },
    },
    async (options) => {
      const binding = createRequire(import.meta.url)(
        '../dist/guest/agw_node_guest.js',
      ) as {
        Guest: new (
          selection: string,
          now: number,
        ) => Parameters<typeof bindGuest>[0];
      };
      return bindGuest(
        new binding.Guest(JSON.stringify(options.selection), performance.now()),
        options,
      );
    },
  );
  await sdk.ready;
  const payload = { value: 100 };
  const upstream = vi.fn(async () => payload);
  await expect(
    sdk.guard(upstream, { tool: 'op', operation: 'write' })(),
  ).rejects.toMatchObject({
    name: 'AgindowDriftError',
    phase: 'response',
    driftClass: 'semantic',
    outcome: payload,
  });
  expect(upstream).toHaveBeenCalledTimes(1);
  const plain = createAgindow();
  await plain.ready;
  expect(await plain.guard(async () => payload, { operation: 'read' })()).toBe(
    payload,
  );
  expect(network).not.toHaveBeenCalled();
});
