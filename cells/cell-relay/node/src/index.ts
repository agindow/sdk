import { repairOptions } from './selective.js';
import { unavailableAudit } from './audit.js';
import type { AuditSink } from './audit.js';
export type { AuditSink, AuditRecord, AuditDrain } from './audit.js';
import { guard as primitive } from './guard.js';
import type { Snapshot, InspectionHooks } from './guard.js';
import { loadWasm } from './wasm.js';
import { resolveConfig, validate, warn } from './config.js';
import { Events } from './events.js';
import { tools, client, detect, ambiguity } from './adapters.js';
import { http } from './http.js';
import { simulate as dryRun } from './simulate.js';
import type { SimulationInput } from './simulate.js';
import type { ClientOptions } from './adapters.js';
import { AgindowSetupError } from './types.js';
import type { Callable, Config, Engine, EngineLoader, GuardOptions, Options } from './types.js';
export * from './types.js';
export type { ToolLike, ToolMap, ToolArray, ClientOptions } from './adapters.js';
export type { SimulationInput, Simulation } from './simulate.js';
export type { SandboxContract, SandboxFields, SandboxResult } from './sandbox.js';

export function createAgindow(options: Options = {}, load: EngineLoader = loadWasm) {
  if (options.audit !== undefined) {
    const { tenant, capacity = 64, maxBytes = 524288 } = options.audit;
    if (typeof tenant !== 'string' || tenant.length === 0 || Buffer.byteLength(tenant) > 256 || /[\u0000-\u001f\u007f]/.test(tenant)
      || !Number.isSafeInteger(capacity) || capacity < 1 || capacity > 256
      || !Number.isSafeInteger(maxBytes) || maxBytes < 1024 || maxBytes > 1048576) throw new AgindowSetupError('invalid audit options');
  }
  if (options.selection?.materiality !== undefined && (!Number.isFinite(options.selection.materiality)
    || options.selection.materiality <= 0 || options.selection.materiality > 1)) throw new AgindowSetupError('invalid materiality');
  if (options.audit !== undefined && options.selection !== undefined && options.audit.tenant !== options.selection.tenant) {
    throw new AgindowSetupError('audit tenant must match selected scope');
  }
  const selectiveRepair = repairOptions(options.selectiveRepair, options.audit !== undefined);
  if (selectiveRepair !== undefined && load !== loadWasm) throw new AgindowSetupError('selectiveRepair requires the bundled engine');
  let initial = resolveConfig(options);
  if (options.work !== undefined && (!Number.isSafeInteger(options.work) || options.work < 0 || options.work > 0xffff_ffff)) {
    throw new AgindowSetupError('work must be a u32 allowance');
  }
  let current: Engine | undefined;
  let revision = 0;
  const events = new Events(); events.resize(initial.buffer ?? 100);
  const latest = new Map<keyof Config, { config: Config; revision: number }>();
  const ready = Promise.resolve().then(() => load({ ...options, ...initial, ...(selectiveRepair === undefined ? {} : { selectiveRepair }) })).then(engine => { current = engine; }, () => {});
  const make = <F extends Callable>(fn: F, local: GuardOptions = {}, hooks?: InspectionHooks): F => {
    validate(local);
    if (typeof fn !== 'function') throw new AgindowSetupError('guard requires a callable');
    if (local.tool !== undefined && typeof local.tool !== 'string') throw new AgindowSetupError('tool must be a string');
    if (local.toolName !== undefined && typeof local.toolName !== 'function') throw new AgindowSetupError('toolName must be callable');
    if (local.inputShape !== undefined && !['raw', 'parsed'].includes(local.inputShape)) throw new AgindowSetupError('inputShape must be raw or parsed');
    if (local.operation !== undefined && !['read', 'write', 'unknown'].includes(local.operation)) {
      throw new AgindowSetupError('operation must be read, write or unknown');
    }
    const created = revision;
    return primitive(fn, local, (): Snapshot => {
      let config: Config = { ...initial, ...local };
      for (const update of latest.values()) if (update.revision > created) config = { ...config, ...update.config };
      const callback = config.onEvent;
      return { ...config, mode: config.mode ?? 'observe', failClosed: config.failClosed ?? true,
        onEvent(event) {
          events.emit(event);
          try { callback?.(event); } catch { /* best effort */ }
        } };
    }, () => current, hooks);
  };
  const toolAdapter = <T extends object>(target: T, config: GuardOptions = {}): T => {
    validate(config); return tools(target, make, config);
  };
  const clientAdapter = <T extends object>(target: T, config: ClientOptions = {}): T => {
    validate(config); return client(target, make, config);
  };
  const httpAdapter = <T extends object>(target: T, config: GuardOptions = {}): T => {
    validate(config); return http(target, make, config);
  };
  return {
    ready,
    async drainAudit(sink: AuditSink, limit = 64) {
      try { return await current?.drainAudit?.(sink, limit) ?? unavailableAudit; }
      catch { return unavailableAudit; }
    },
    configure(config: Config): void {
      validate(config);
      if (config.failClosed === false) warn('fail-open writes requested. Semantic writes remain protected in repair mode; strict mode ignores this opt-out.');
      revision++; initial = { ...initial, ...config };
      for (const key of Object.keys(config) as (keyof Config)[]) {
        latest.set(key, { config: { [key]: config[key] }, revision });
      }
      events.resize(initial.buffer ?? 100);
    },
    guard: make, toolAdapter, clientAdapter, httpAdapter,
    on: events.on.bind(events), recent: events.recent.bind(events),
    simulate(tool: string, input: SimulationInput) {
      validate(input);
      return dryRun(tool, input, { ...initial, mode: initial.mode ?? 'observe', failClosed: initial.failClosed ?? true }, current);
    },
    wrap<T extends object>(target: T, config: ClientOptions = {}): T {
      validate(config);
      if (target === null) throw new AgindowSetupError('expected a callable, tool map/array or client');
      let candidates: ReturnType<typeof detect>;
      try { candidates = detect(target); }
      catch { warn('cannot detect adapter; leaving target unchanged'); return target; }
      if (candidates.length !== 1) ambiguity(candidates);
      if (candidates[0] === 'clientAdapter') return clientAdapter(target, config);
      if (typeof config.tool === 'function') throw new AgindowSetupError('dynamic tool requires clientAdapter or guard toolName');
      const { tool, ...rest } = config;
      const local: GuardOptions = { ...rest, ...(tool === undefined ? {} : { tool }) };
      if (candidates[0] === 'httpAdapter') return httpAdapter(target, local);
      if (typeof target === 'function') return make(target as Callable, local) as T;
      return toolAdapter(target, local);
    },
  };
}
export const agindow = createAgindow();
export const guard = agindow.guard;
export const wrap = agindow.wrap;
export const toolAdapter = agindow.toolAdapter;
export const clientAdapter = agindow.clientAdapter;
export const httpAdapter = agindow.httpAdapter;
export const simulate = agindow.simulate;
export { guardRemote } from './remote.js';
export type { RemoteEnvelope, RemoteOptions } from './remote.js';

export { createRemoteAuth, createProofSigner } from './machine-auth.js';
export type { RemoteAuth, RemoteAuthOptions, AccessCredential, ProofSigner } from './machine-auth.js';
export { workloadIdentity } from './identity-source.js';
export type { WorkloadIdentityOptions } from './identity-source.js';
