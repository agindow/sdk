/** TI-49 WASM host transport. Call only for detached background Job bodies;
 * pass bytes/errors back to Rust Cache.complete. Never attach to guard evaluation.
 * A normal HTTPS fetch is sufficient; no Cloudflare binding is required.
 */
export function createSnapshotTransport(endpoint: string, tenant: string, token: string,
  fetcher: (request: Request) => Promise<Response> = request => fetch(request)):
  (body: string, timeoutMs: number) => Promise<Uint8Array> {
  const url = new URL(endpoint);
  if (url.protocol !== 'https:' || url.username || url.password || url.search || url.hash
    || url.pathname !== '/profile-snapshots' || endpoint.length > 4096
    || !tenant || tenant.length > 256 || !/^[A-Za-z0-9\-_.~]{32,512}$/.test(token)) throw new Error('Invalid snapshot transport');
  return async (body, timeoutMs) => {
    if (!Number.isSafeInteger(timeoutMs) || timeoutMs < 1 || timeoutMs > 30000
      || new TextEncoder().encode(body).length > 8192) throw new Error('Refresh limits');
    const selected: unknown = JSON.parse(body);
    if (typeof selected !== 'object' || selected === null || !('scope' in selected)
      || typeof selected.scope !== 'object' || selected.scope === null || !('tenant' in selected.scope)
      || selected.scope.tenant !== tenant) throw new Error('Refresh tenant');
    const controller = new AbortController();
    let reader: ReadableStreamDefaultReader<Uint8Array> | undefined;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const task = async (): Promise<Uint8Array> => {
      const response = await fetcher(new Request(url, { method: 'POST', body, redirect: 'error', signal: controller.signal,
        headers: { Authorization: `Bearer ${token}`, 'Content-Type': 'application/json' } }));
      if (controller.signal.aborted || response.status !== 200 || response.body === null) {
        await response.body?.cancel(); throw new Error('Snapshot unavailable');
      }
      reader = response.body.getReader();
      const chunks: Uint8Array[] = [];
      let length = 0;
      while (true) {
        const { done, value } = await reader.read();
        if (done) break;
        length += value.length;
        if (length > 131072) { await reader.cancel(); throw new Error('Snapshot byte cap'); }
        chunks.push(value);
      }
      const bytes = new Uint8Array(length);
      let offset = 0;
      for (const chunk of chunks) { bytes.set(chunk, offset); offset += chunk.length; }
      return bytes;
    };
    // Race also bounds a stalled response stream or a host fetch ignoring abort.
    // Install an observer on task via race so late rejection is always handled.
    try {
      return await Promise.race([task(), new Promise<never>((_resolve, reject) => {
        timer = setTimeout(() => {
          controller.abort(); void reader?.cancel().catch(() => {}); reject(new Error('Snapshot timeout'));
        }, timeoutMs);
      })]);
    } finally { if (timer !== undefined) clearTimeout(timer); }
  };
}
