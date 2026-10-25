import type { IncomingMessage, ServerResponse } from 'node:http';
import { AgindowDriftError } from './types.js';
import type { GuardOptions } from './types.js';
import type { GuardFactory } from './adapters.js';
import { RAW_CAP, RawJson } from './raw.js';
interface Completed {
  readonly response: ServerResponse;
  readonly args: readonly unknown[];
  readonly sent: boolean;
  readonly raw?: RawJson;
}
type Next = (error?: unknown) => void;
function json(response: ServerResponse): boolean {
  const header = response.getHeader('content-type');
  return typeof header === 'string' && /^application\/(?:json|[\w.+-]+\+json)(?:;|$)/i.test(header)
    && response.getHeader('transfer-encoding') === undefined;
}
function payload(error: AgindowDriftError): string {
  return JSON.stringify({ error: error.code, tool: error.tool, drift_class: error.driftClass,
    drift_id: error.driftId, hint: error.hint, audit: error.auditUrl });
}
/** Intercept only end-with-complete-JSON. write()/flushHeaders() preserve
 * streaming behavior and deliberately prevent response inspection. */
export function expressAdapter<T extends object>(target: T, make: GuardFactory, config: GuardOptions): T {
  const run = make(function(this: unknown, request: IncomingMessage, response: ServerResponse, next?: Next): Promise<Completed> {
    return new Promise((resolve, reject) => {
      const end = response.end; const write = response.write;
      let streamed = false; let settled = false;
      const restore = (): void => {
        response.end = end; response.write = write;
        response.off('close', closed);
      };
      const closed = (): void => {
        if (settled) return; settled = true; restore();
        resolve({ response, args: [], sent: true });
      };
      response.once('close', closed);
      Reflect.set(response, 'write', function(...args: unknown[]): unknown {
        streamed = true; return Reflect.apply(write, response, args);
      });
      Reflect.set(response, 'end', function(...args: unknown[]): unknown {
        if (settled) return Reflect.apply(end, response, args);
        settled = true; restore();
        const chunk = args[0]; const encoding = args[1];
        let raw: RawJson | undefined;
        if (!streamed && !response.headersSent && json(response)) {
          try {
            const bytes = typeof chunk === 'string' ? Buffer.from(chunk, typeof encoding === 'string' ? encoding as BufferEncoding : 'utf8')
              : chunk instanceof Uint8Array ? Buffer.from(chunk) : undefined;
            if (bytes !== undefined && bytes.byteLength <= RAW_CAP) raw = new RawJson(bytes);
          } catch { /* unsupported capture passes through */ }
        }
        const sent = raw === undefined;
        if (sent) Reflect.apply(end, response, args);
        resolve({ response, args, sent, ...(raw === undefined ? {} : { raw }) });
        return response;
      });
      const onNext: Next = error => {
        if (settled) { next?.(error); return; }
        settled = true; restore();
        if (error !== undefined) reject(error);
        else { next?.(); resolve({ response, args: [], sent: true }); }
      };
      try { Reflect.apply(target as (...args: never[]) => unknown, this, [request, response, onNext]); }
      catch (error) { settled = true; restore(); reject(error); }
    });
  }, config, {
    request(args) {
      const request = args[0];
      return request !== null && typeof request === 'object' ? Object.getOwnPropertyDescriptor(request, 'body')?.value ?? {} : {};
    },
    operation(args) {
      const request = args[0];
      const method: unknown = request !== null && typeof request === 'object' ? Object.getOwnPropertyDescriptor(request, 'method')?.value : undefined;
      return typeof method === 'string' && ['GET', 'HEAD', 'OPTIONS'].includes(method.toUpperCase()) ? 'read' : 'unknown';
    },
    response(value) {
      const completed = value as Completed;
      return { inspect: completed?.raw !== undefined, value: completed?.raw };
    },
    replaceRequest() { throw Error('raw incoming request repair needs a host-provided body boundary'); },
    replaceResponse(original, value) {
      const completed = original as Completed;
      const bytes = value instanceof RawJson ? value.bytes : value instanceof Uint8Array ? value : undefined;
      if (bytes === undefined || completed.raw === undefined) throw Error('raw repair required');
      if (Buffer.from(bytes).equals(Buffer.from(completed.raw.bytes))) return completed;
      for (const header of ['content-length', 'etag', 'content-encoding', 'content-md5']) completed.response.removeHeader(header);
      completed.response.setHeader('content-length', bytes.byteLength);
      return { ...completed, args: [Buffer.from(bytes), ...completed.args.slice(typeof completed.args[1] === 'string' ? 2 : 1)] } satisfies Completed;
    },
  });
  const wrapped = function(this: unknown, request: IncomingMessage, response: ServerResponse, next?: Next): void {
    const fail = (error: unknown): void => {
      if (next !== undefined) { next(error); return; }
      if (response.headersSent || response.destroyed) return;
      if (error instanceof AgindowDriftError) {
        const body = payload(error); response.statusCode = 409;
        response.setHeader('content-type', 'application/json'); response.setHeader('content-length', Buffer.byteLength(body)); response.end(body);
      } else { response.statusCode = 500; response.end(); }
    };
    try {
      void Reflect.apply(run, this, [request, response, next]).then(completed => {
        if (!completed.sent && !response.destroyed) Reflect.apply(response.end, response, [...completed.args]);
      }, fail);
    } catch (error) { fail(error); }
  };
  Object.defineProperties(wrapped, Object.getOwnPropertyDescriptors(target));
  return wrapped as T;
}
