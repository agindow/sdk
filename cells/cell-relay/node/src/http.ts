import { warn } from './config.js';
import { RawJson, RAW_CAP } from './raw.js';
import type { GuardFactory } from './adapters.js';
import type { Callable, GuardOptions } from './types.js';
import { expressAdapter } from './http-node.js';

function inspectable(response: Response): boolean {
  const type = response.headers.get('content-type')?.split(';')[0]?.trim().toLowerCase();
  const size = response.headers.get('content-length');
  return type !== undefined && (type === 'application/json' || /^application\/[\w.+-]+\+json$/.test(type))
    && !response.headers.has('transfer-encoding') && size !== null && /^\d+$/.test(size) && Number(size) <= RAW_CAP;
}
async function read(response: Response): Promise<RawJson | undefined> {
  const reader = response.clone().body?.getReader();
  if (reader === undefined) return new RawJson(new Uint8Array());
  const parts: Uint8Array[] = []; let length = 0;
  try {
    for (;;) {
      const chunk = await reader.read();
      if (chunk.done) break;
      length += chunk.value.byteLength;
      if (length > RAW_CAP) { void reader.cancel().catch(() => {}); return undefined; }
      parts.push(chunk.value);
    }
    const bytes = new Uint8Array(length); let offset = 0;
    for (const part of parts) { bytes.set(part, offset); offset += part.byteLength; }
    return new RawJson(bytes);
  } finally { reader.releaseLock(); }
}
/** Fetch adapter: completed bounded JSON is inspected without replacing the
 * original Response. Streaming and unknown-length bodies are never touched. */
export function http<T extends object>(target: T, make: GuardFactory, config: GuardOptions): T {
  if (typeof target !== 'function') { warn('httpAdapter requires fetch or an Express callable; leaving target unchanged'); return target; }
  try {
    if (Object.getOwnPropertyDescriptor(target, 'use') !== undefined) {
      return expressAdapter(target, make, config);
    }
    const captured = new WeakMap<Response, RawJson>();
    const failedCapture = new WeakSet<Response>();
    const upstream = function(this: unknown, ...args: unknown[]): unknown {
      const value: unknown = Reflect.apply(target, this, args);
      const prepare = async (response: unknown): Promise<unknown> => {
        if (response instanceof Response) {
          try {
            if (inspectable(response)) {
              const raw = await read(response);
              if (raw !== undefined) captured.set(response, raw);
              else failedCapture.add(response);
            }
          } catch { failedCapture.add(response); }
        }
        return response;
      };
      return value instanceof Promise ? value.then(prepare) : value;
    };
    const guarded = make(upstream, config, {
      request(args) {
        const init = args[1];
        const body: unknown = init !== null && typeof init === 'object' ? Object.getOwnPropertyDescriptor(init, 'body')?.value : undefined;
        if (typeof body !== 'string' && !(body instanceof Uint8Array)) return {};
        if (Buffer.byteLength(body) > RAW_CAP) throw Error('request inspection cap exceeded');
        return new RawJson(Buffer.from(body));
      },
      operation(args) {
        const init = args[1];
        const method: unknown = init !== null && typeof init === 'object' ? Object.getOwnPropertyDescriptor(init, 'method')?.value : undefined;
        const verb = typeof method === 'string' ? method : args[0] instanceof Request ? args[0].method : 'GET';
        return ['GET', 'HEAD', 'OPTIONS'].includes(verb.toUpperCase()) ? 'read' : 'unknown';
      },
      response(value) {
        if (value instanceof Response && failedCapture.has(value)) throw Error('response inspection unavailable');
        const raw = value instanceof Response ? captured.get(value) : undefined;
        return { inspect: raw !== undefined, value: raw };
      },
      replaceRequest(args, value) {
        if (!(value instanceof Uint8Array) && !(value instanceof RawJson)) throw Error('raw repair required');
        const bytes = value instanceof RawJson ? value.bytes : value;
        const init = args[1];
        if (init !== undefined && (typeof init !== 'object' || init === null)) throw Error('unsupported request init');
        const original: unknown = init === undefined ? undefined : Object.getOwnPropertyDescriptor(init, 'body')?.value;
        if (typeof original !== 'string' && !(original instanceof Uint8Array)) throw Error('original raw body unavailable');
        const source: unknown = init === undefined ? undefined : Object.getOwnPropertyDescriptor(init, 'headers')?.value;
        const headers = new Headers(source as HeadersInit | undefined ?? (args[0] instanceof Request ? args[0].headers : undefined));
        const encoding = headers.get('content-encoding');
        if (encoding !== null && encoding.trim().toLowerCase() !== 'identity') throw Error('encoded request repair unavailable');
        for (const key of ['content-length', 'content-md5', 'digest']) headers.delete(key);
        headers.set('content-length', String(bytes.byteLength));
        return [args[0], { ...init, headers, body: Buffer.from(bytes) }];
      },
      replaceResponse(original, value) {
        if (!(original instanceof Response) || (!(value instanceof Uint8Array) && !(value instanceof RawJson))) throw Error('raw repair required');
        const bytes = value instanceof RawJson ? value.bytes : value;
        const prior = captured.get(original);
        if (prior !== undefined && Buffer.from(prior.bytes).equals(Buffer.from(bytes))) return original;
        const headers = new Headers(original.headers);
        for (const key of ['content-length', 'content-encoding', 'etag', 'content-md5', 'digest']) headers.delete(key);
        headers.set('content-length', String(bytes.byteLength));
        return new Response(Buffer.from(bytes), { status: original.status, statusText: original.statusText, headers });
      },
    });
    Object.defineProperties(guarded, Object.getOwnPropertyDescriptors(target));
    return guarded as T;
  } catch { warn('cannot install httpAdapter; leaving target unchanged'); return target; }
}
