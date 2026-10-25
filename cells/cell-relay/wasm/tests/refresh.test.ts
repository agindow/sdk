import { expect, it } from 'vitest';
import { createSnapshotTransport } from '../refresh.js';
it('uses a host HTTPS fetch without a Worker binding, bounded response and no redirects', async () => {
  const calls: Request[] = [];
  const transport = createSnapshotTransport('https://registry.test/profile-snapshots', 'tenant-a',
    'test-reader-credential-with-32-bytes', async request => {
      calls.push(request);
      return new Response('{"ok":true}');
    });
  const body = JSON.stringify({ scope: { tenant: 'tenant-a' } });
  expect(new TextDecoder().decode(await transport(body, 100))).toBe('{"ok":true}');
  expect(calls[0]?.redirect).toBe('error');
  expect(calls[0]?.headers.get('Authorization')).toBe('Bearer test-reader-credential-with-32-bytes');
  await expect(transport(JSON.stringify({ scope: { tenant: 'victim' } }), 100)).rejects.toThrow();
  expect(calls).toHaveLength(1);
  expect(() => createSnapshotTransport('http://registry.test/profile-snapshots', 't', 'secret')).toThrow();
});
it('fails offline, timed-out, oversized and non-success responses', async () => {
  const make = (fetcher: (request: Request) => Promise<Response>) => createSnapshotTransport(
    'https://registry.test/profile-snapshots', 'tenant-a', 'test-reader-credential-with-32-bytes', fetcher);
  const body = JSON.stringify({ scope: { tenant: 'tenant-a' } });
  await expect(make(async () => new Response('moved', { status: 302 }))(body, 100)).rejects.toThrow();
  await expect(make(async () => { throw new Error('offline'); })(body, 100)).rejects.toThrow();
  await expect(make(async () => new Response('x'.repeat(131073)))(body, 100)).rejects.toThrow();
  let aborted = false;
  await expect(make(request => new Promise((_resolve, reject) => {
    request.signal.addEventListener('abort', () => { aborted = true; reject(new Error('aborted')); });
  }))(body, 5)).rejects.toThrow();
  expect(aborted).toBe(true);
  await expect(make(async () => new Response(new ReadableStream({ start() {} })))(body, 5)).rejects.toThrow();
});
