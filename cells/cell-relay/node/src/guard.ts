import { AgindowDriftError } from './types.js';
import type { Callable, Engine, EngineCall, GuardOptions, Mode, Verdict } from './types.js';
import { performance } from 'node:perf_hooks';
import { openSpan } from './otel.js';
import { types } from 'node:util';
import { warn } from './config.js';
import { RawJson, RAW_CAP, restoreRaw } from './raw.js';
import type { Operation } from './types.js';
export interface Snapshot extends GuardOptions { readonly mode: Mode; readonly failClosed: boolean }
export interface InspectionHooks {
  request(args: readonly unknown[]): unknown;
  response(value: unknown): { readonly inspect: boolean; readonly value: unknown };
  replaceRequest?(args: readonly unknown[], value: unknown): readonly unknown[];
  replaceResponse?(original: unknown, value: unknown): unknown;
  operation?(args: readonly unknown[]): Operation;
}
export function guard<F extends Callable>(fn: F, options: GuardOptions,
  snapshot: () => Snapshot, engine: () => Engine | undefined, hooks?: InspectionHooks): F {
  function wrapped(this: unknown, ...args: unknown[]): unknown {
    const config = snapshot();
    const started = performance.now();
    let tool = options.tool ?? (fn.name || 'tool');
    let identityFault = false;
    try { if (options.toolName !== undefined) tool = Reflect.apply(options.toolName, this, args); }
    catch { identityFault = true; }
    if (typeof tool !== 'string' || tool.length === 0) { tool = fn.name || 'tool'; identityFault = true; }
    const span = openSpan(config.spans !== false, tool, config.mode);
    let operation = options.operation ?? 'read';
    try { if (options.operation === undefined && hooks?.operation !== undefined) operation = hooks.operation(args); }
    catch { identityFault = true; }
    let bypassed = false;
    let errored = false;
    let call: EngineCall | undefined;
    let closed = false;
    const emit = (type: 'bypass' | 'drift' | 'blocked' | 'repair' | 'error', phase: 'request' | 'response', verdict?: Verdict, byteIdentical?: boolean): void => {
      // Copy only bounded metadata; a listener must never mutate engine state.
      let frozen: Verdict | undefined;
      if (verdict !== undefined) {
        const { repair: _repair, ...metadata } = verdict;
        frozen = Object.freeze({ ...metadata,
          ...(verdict.feedback === undefined ? {} : { feedback: Object.freeze({ ...verdict.feedback }) }) });
      }
      const event = Object.freeze({ type, tool, mode: config.mode, phase, side: phase,
        latencyUs: Math.max(0, (performance.now() - started) * 1000),
        ...(frozen === undefined ? {} : { verdict: frozen }),
        ...(verdict?.class == null ? {} : { driftClass: verdict.class }),
        ...(verdict?.confidence === undefined ? {} : { confidence: verdict.confidence }),
        ...(verdict?.auditId === undefined ? {} : { auditId: verdict.auditId }),
        ...(byteIdentical === undefined ? {} : { byte_identical: byteIdentical }) });
      span.event(event);
      try { config.onEvent?.(event); } catch { /* best effort */ }
    };
    const bypass = (phase: 'request' | 'response', internal = false): void => {
      if (internal && !errored) { errored = true; emit('error', phase); }
      if (!bypassed) { bypassed = true; emit('bypass', phase); }
    };
    const close = (): void => {
      if (closed) return;
      closed = true;
      try { call?.close(); } catch { bypass('response', true); }
    };
    const apply = (verdict: Verdict, phase: 'request' | 'response', value?: unknown, cause?: unknown): unknown => {
      if (verdict.bypassed) { bypass(phase); return value; }
      const halted = verdict.halt && config.mode !== 'observe';
      let effective = value;
      const patch = verdict.repair;
      if (!halted && patch !== undefined && config.mode !== 'observe') {
        try {
          const threshold = config.mode === 'strict' || verdict.class === 'semantic' ? 0.99 : 0.97;
          if (patch.verified !== true || verdict.class === 'unknown' || verdict.class === null || verdict.confidence === undefined
            || !Number.isFinite(verdict.confidence) || verdict.confidence < threshold || verdict.confidence > 1) throw Error('invalid repair evidence');
          effective = phase === 'request' ? patch.value : hooks?.replaceResponse === undefined ? patch.value : hooks.replaceResponse(value, patch.value);
          if (phase === 'request' && hooks === undefined && options.inputShape === 'raw') effective = restoreRaw(value, effective);
          if (phase === 'request' && effective !== value) {
            if (hooks?.replaceRequest !== undefined) args = [...hooks.replaceRequest(args, effective)];
            else if (args.length <= 1) args = [effective];
            else if (Array.isArray(effective) && effective.length === args.length) args = effective;
            else throw Error('repair does not match argument shape');
          }
        } catch { bypass(phase, true); return value; }
      }
      if (verdict.findings > 0) emit('drift', phase, verdict);
      if (halted) {
        emit('blocked', phase, verdict);
        span.end();
        throw new AgindowDriftError(tool, config.mode, phase, verdict, value, cause);
      }
      if (effective !== value && config.mode !== 'observe') emit('repair', phase, verdict,
        value instanceof Uint8Array && effective instanceof Uint8Array ? Buffer.from(value).equals(Buffer.from(effective)) : false);
      return effective;
    };
    try {
      const scopedOut = config.denyTools?.includes(tool) === true
        || (config.allowedTools !== undefined && config.allowedTools !== '*' && !config.allowedTools.includes(tool));
      const selected = identityFault || scopedOut ? undefined : engine();
      if (selected === undefined) bypass('request', identityFault);
      else {
        let input = hooks === undefined ? (args.length === 1 ? args[0] : args.length === 0 ? {} : args) : hooks.request(args);
        if (hooks === undefined && options.inputShape === 'raw') {
          if ((typeof input !== 'string' && !(input instanceof Uint8Array)) || Buffer.byteLength(input) > RAW_CAP) throw Error('raw inspection unavailable');
          input = new RawJson(Buffer.from(input));
        }
        call = selected.begin(input, config.mode, operation, config.failClosed, tool);
      }
    } catch { bypass('request', true); }
    let request: Verdict | undefined;
    try { request = call?.request; } catch { bypass('request', true); }
    if (request !== undefined) {
      if (request.halt && !request.bypassed && config.mode !== 'observe') close();
      if (!bypassed) {
        const original = args.length === 1 ? args[0] : args;
        apply(request, 'request', original);
      }
    }
    const finish = (value: unknown, failed: boolean): unknown => {
      let decision: Verdict | undefined;
      if (call !== undefined && !bypassed) {
        try {
          const projection = hooks?.response(value);
          if (projection === undefined || projection.inspect) decision = call.finish(projection === undefined ? value : projection.value);
        } catch { bypass('response', true); }
      }
      close();
      let effective = value;
      if (decision !== undefined && !bypassed) effective = apply(decision, 'response', failed ? undefined : value, failed ? value : undefined);
      span.end(failed);
      if (failed) throw value;
      return effective;
    };
    let value: unknown;
    try { value = Reflect.apply(fn, this, args); }
    catch (error) { return finish(error, true); }
    // Ordinary functions returning promises must be inspected after resolution;
    // constructor-name checks miss fetch/undici/axios wrappers.
    if (types.isPromise(value)) {
      try { return Promise.prototype.then.call(value, v => finish(v, false), e => finish(e, true)); }
      catch { bypass('response', true); close(); span.end(); return value; }
    }
    return finish(value, false);
  }
  try { Object.defineProperties(wrapped, Object.getOwnPropertyDescriptors(fn)); }
  catch { warn('cannot preserve callable metadata; leaving callable unchanged'); return fn; }
  return wrapped as unknown as F;
}
