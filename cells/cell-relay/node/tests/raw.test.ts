import { expect, it, vi } from 'vitest';
import { createAgindow } from '../src/index.js';
import { capture } from '../src/inspection.js';
import type { Verdict } from '../src/types.js';
const pass: Verdict = { halt: false, bypassed: false, class: null, findings: 0, detectors: 0, malformed: false };
it('raw input preserves numeric lexemes and BOM for inspection without replacing arguments', async () => {
  const seen: string[] = [];
  const sdk = createAgindow({}, async () => ({ begin(input) { seen.push(capture(input).json); return { request: pass, finish: () => pass, close() {} }; } })); await sdk.ready;
  const original = '\uFEFF {"id":9007199254740993}';
  expect(sdk.guard((input: string) => input, { inputShape: 'raw' })(original)).toBe(original);
  expect(seen).toEqual([original]);
});
it('HTTP methods default conservatively and oversized input bypasses before allocation/evaluation', async () => {
  const operations: string[] = []; let begins = 0;
  const sdk = createAgindow({}, async () => ({ begin(_value, _mode, operation) { begins++; operations.push(operation); return { request: pass, finish: () => pass, close() {} }; } })); await sdk.ready;
  const original = new Response('stream');
  const upstream = vi.fn(async (_url: string, _init?: RequestInit) => original);
  const wrapped = sdk.httpAdapter(upstream);
  await wrapped('https://unused.invalid'); await wrapped('https://unused.invalid', { method: 'POST' });
  expect(operations).toEqual(['read', 'unknown']);
  await wrapped('https://unused.invalid', { body: 'x'.repeat(1024 * 1024 + 1) });
  expect(begins).toBe(2); expect(upstream).toHaveBeenCalledTimes(3);
  expect(sdk.recent().filter(event => event.type === 'bypass')).toHaveLength(1);
});
it('rejects deterministic invalid guard options at setup', () => {
  const sdk = createAgindow();
  for (const options of [{ tool: 2 }, { toolName: 2 }, { inputShape: 'guess' }]) {
    expect(() => sdk.guard(() => 1, options as never)).toThrow();
  }
});

it('raw calls preserve original bytes and dispatch once during unavailable inspection', async () => {
  for (const fault of ['throws', 'warming']) {
    const sdk = createAgindow({ mode: 'repair' }, async () => ({ begin() {
      if (fault === 'throws') throw Error('engine fault');
      return { request: { ...pass, bypassed: true }, finish: () => pass, close() {} };
    } }));
    await sdk.ready;
    const input = Buffer.from('{"old_name":"Ada"}');
    const upstream = vi.fn((value: Buffer) => value);
    expect(sdk.guard(upstream, { inputShape: 'raw' })(input)).toBe(input);
    expect(upstream).toHaveBeenCalledExactlyOnceWith(input);
    expect(sdk.recent().filter(event => event.type === 'bypass')).toHaveLength(1);
  }
});
