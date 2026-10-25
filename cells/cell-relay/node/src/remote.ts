import { validLegacyKey } from './machine-auth.js';
import type { RemoteAuth } from './machine-auth.js';
import { performance } from 'node:perf_hooks';
import { AgindowDriftError, AgindowSetupError } from './types.js';
import type { Event, Mode, Operation, PolicyFeedback, Verdict } from './types.js';

/** Execution envelope; credentials belong to the connection binding. */
export interface RemoteEnvelope {
  readonly tool: string;
  readonly connection: string;
  readonly operation: Operation;
  readonly args: unknown;
  readonly mode: Mode;
  readonly idempotency_key?: string;
  readonly metadata?: Readonly<Record<string, unknown>>;
}
export interface RemoteOptions {
  readonly endpoint: string;
  readonly apiKey?: string;
  readonly auth?: RemoteAuth;
  readonly timeoutMs?: number;
  readonly onAuthDiagnostic?: (reason:'legacy_unbound') => void;
  readonly onEvent?: (event: Event) => void;
}
function feedback(value: unknown, input: RemoteEnvelope): PolicyFeedback | undefined {
  if (value === null || typeof value !== 'object' || Array.isArray(value)) return undefined;
  const fields = Object.entries(value);
  const keys = ['error', 'tool', 'drift_class', 'drift_id', 'hint', 'audit'];
  if (fields.length !== keys.length || fields.some(([key, entry]) => !keys.includes(key) || typeof entry !== 'string')) return undefined;
  const item = value as Record<string, string>;
  if (input.mode === 'observe' || item['tool'] !== input.tool
    || item['error'] !== (input.mode === 'strict' ? 'drift_halted' : 'drift_blocked')
    || !['shape', 'semantic', 'unknown'].includes(item['drift_class'] ?? '')
    || !item['drift_id'] || !item['audit']) return undefined;
  // One cast after validating the closed wire shape.
  return value as PolicyFeedback;
}
async function readFeedback(response: Response): Promise<unknown> {
  const reader = response.body?.getReader();
  if (reader === undefined) return undefined;
  let length = 0; const chunks: Uint8Array[] = [];
  try {
    for (;;) {
      const chunk = await reader.read();
      if (chunk.done) break;
      length += chunk.value.byteLength;
      if (length > 16_384) throw Error('feedback exceeds bound');
      chunks.push(chunk.value);
    }
    return JSON.parse(Buffer.concat(chunks, length).toString('utf8')) as unknown;
  } finally { void reader.cancel().catch(() => {}); reader.releaseLock(); }
}
/** Explicit asynchronous remote execution. The fallback binding must honor the
 * same upstream idempotency key: a connection reset does not prove zero writes. */
export async function guardRemote<T>(original: (input: RemoteEnvelope) => T | Promise<T>,
  input: RemoteEnvelope, options: RemoteOptions): Promise<Response | T> {
  if (options.auth !== undefined && options.apiKey !== undefined) throw new AgindowSetupError('configure legacy fallback in the auth session');
  if (options.apiKey !== undefined) {
    if (!validLegacyKey(options.apiKey)) throw new AgindowSetupError('invalid legacy key');
    try { options.onAuthDiagnostic?.('legacy_unbound'); } catch { /* diagnostic only */ }
  }
  const timeout = options.timeoutMs ?? 5000;
  let url: URL;
  try { url = new URL(options.endpoint); } catch { throw new AgindowSetupError('invalid remote endpoint'); }
  if (!['http:', 'https:'].includes(url.protocol) || url.username || url.password
    || !Number.isFinite(timeout) || timeout <= 0 || timeout > 2_147_483_647) throw new AgindowSetupError('invalid remote transport options');
  if (!['read', 'write', 'unknown'].includes(input.operation) || !['observe', 'repair', 'strict'].includes(input.mode)
    || !input.tool || !input.connection) throw new AgindowSetupError('invalid remote envelope');
  if (input.operation !== 'read' && !input.idempotency_key) throw new AgindowSetupError('remote writes require an upstream idempotency key');
  const started = performance.now();
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeout);
  let response: Response | undefined;
  let halt: PolicyFeedback | undefined;
  let completedFailure = false;
  const emitBypass = (phase: 'request' | 'response', reason: string): void => {
    try { options.onEvent?.(Object.freeze({ type: 'bypass', tool: input.tool, mode: input.mode,
      phase, reason, latencyUs: (performance.now() - started) * 1000 })); }
    catch { /* diagnostic sinks never affect upstream */ }
  };
  try {
    const authHeaders = options.auth?.headers('POST', url.href);
    if (options.auth !== undefined && authHeaders === undefined) throw Error('auth unavailable');
    response = await fetch(url, { method: 'POST', redirect: 'error', signal: controller.signal,
      headers: { 'content-type': 'application/json', ...(options.apiKey === undefined ? {} : { authorization: `Bearer ${options.apiKey}` }), ...authHeaders },
      body: JSON.stringify(input) });
    if (response.status >= 500) {
      completedFailure = ['ok', 'degraded', 'repaired', 'bypassed'].includes(response.headers.get('x-agindow-status') ?? '');
      if (!completedFailure) { void response.body?.cancel().catch(() => {}); response = undefined; }
    }
    else if (response.status === 409 && (!response.headers.has('x-agindow-status') || response.headers.get('x-agindow-status') === 'blocked')) {
      halt = feedback(await readFeedback(response), input);
      if (halt === undefined) response = undefined;
    }
  } catch { response = undefined; }
  finally { clearTimeout(timer); }
  if (response === undefined) {
    emitBypass('request', 'guard_transport_unavailable');
    return original(input);
  }
  if (completedFailure) emitBypass('response', 'completed_upstream_failure_preserved');
  if (halt !== undefined) {
    const phase = (response.headers.has('x-agindow-outcome') || response.headers.get('x-agindow-drift-side') === 'response') ? 'response' : 'request';
    const verdict: Verdict = { halt: true, bypassed: false, class: halt.drift_class, findings: 1,
      detectors: 0, malformed: false, feedback: halt };
    let outcome = response;
    const retained = response.headers.get('x-agindow-outcome');
    if (phase === 'response' && retained !== null) {
      try {
        const location = new URL(retained, url);
        const remaining = Math.ceil(timeout - (performance.now() - started));
        if (location.origin === url.origin && location.pathname.startsWith('/v1/outcome/') && !location.username && !location.password && remaining > 0) {
          const recoveryAuth = options.auth?.headers('GET', location.href);
          if (options.auth !== undefined && recoveryAuth === undefined) throw Error('auth unavailable');
          const recovered = await fetch(location, { redirect: 'error', signal: AbortSignal.timeout(remaining),
            headers: recoveryAuth ?? (options.apiKey === undefined ? {} : { authorization: `Bearer ${options.apiKey}` }) });
          if (recovered.headers.get('x-agindow-retained-outcome') === 'true' || recovered.ok) outcome = recovered;
          else void recovered.body?.cancel().catch(() => {});
        }
      } catch { /* A known completed write is never replayed after recovery failure. */ }
    }
    throw new AgindowDriftError(input.tool, input.mode, phase, verdict, outcome);
  }
  return response;
}
