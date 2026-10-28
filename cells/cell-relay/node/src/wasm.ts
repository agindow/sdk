import { repairOptions, selectiveEngine } from './selective.js';
import { randomUUID } from 'node:crypto';
import { validateAudit, unavailableAudit } from './audit.js';
import { fileURLToPath } from 'node:url';
import { performance } from 'node:perf_hooks';
import { captureBytes } from './inspection.js';
import type { Engine, EngineCall, Mode, Operation, Options, Verdict } from './types.js';
interface GuestCall {
  audit_context?(tool: string, ts: string): void;
  request(): string;
  finish(response: Uint8Array, truncated: boolean, now: number): string;
  free(): void;
}
interface Guest {
  prepare_rename?(bytes: Uint8Array, source: string, target: string, confidence: number, mode: string, work: number): string;
  set_ready?(ready: boolean): void;
  enable_audit?(tenant: string, namespace: string, capacity: number, maxBytes: number): void;
  audit_peek?(): string;
  audit_ack?(id: string): boolean;
  audit_stats?(): string;
  active(): boolean;
  begin(request: Uint8Array, truncated: boolean, mode: string, operation: string, now: number,
    failClosed: boolean, work: number): GuestCall;
}
interface Binding { Guest: new (selection: string, now: number) => Guest }
const EMPTY_CAPTURE: { bytes: Uint8Array; truncated: boolean } = { bytes: new Uint8Array([0x7b, 0x7d]), truncated: false };
function verdict(json: string): Verdict {
  const value: unknown = JSON.parse(json);
  if (typeof value !== 'object' || value === null
    || !('halt' in value) || typeof value.halt !== 'boolean'
    || !('bypassed' in value) || typeof value.bypassed !== 'boolean'
    || !('class' in value) || ![null, 'shape', 'semantic', 'unknown'].includes(value.class as string | null)
    || !('findings' in value) || typeof value.findings !== 'number' || !Number.isSafeInteger(value.findings) || value.findings < 0
    || !('detectors' in value) || typeof value.detectors !== 'number' || !Number.isSafeInteger(value.detectors) || value.detectors < 0
    || !('malformed' in value) || typeof value.malformed !== 'boolean') throw Error('invalid guest verdict');
  const metadata = value as Record<string, unknown>;
  for (const key of ['detectorKinds', 'eligibleDetectors', 'suppressedDetectors']) {
    const entries = metadata[key];
    if (entries !== undefined) {
      if (!Array.isArray(entries) || entries.length > 256 || entries.some(kind =>
        !['Unit', 'Enumeration', 'Completeness', 'TypeConsistency', 'Charset'].includes(String(kind)))) throw Error('invalid guest diagnostics');
      Object.freeze(entries);
    }
  }
  if (metadata['fault'] !== undefined && metadata['fault'] !== null && typeof metadata['fault'] !== 'string') throw Error('invalid guest fault');
  return Object.freeze(value as Verdict);
}
/** Each loader creates an isolated guest instance, including after retirement. */
export async function loadWasm(options: Options): Promise<Engine> {
  // A fresh CommonJS module is required for independent linear memories. The
  // generated module's constructor closes over its instance, so clone via a
  // fresh module compilation rather than sharing a trapped singleton.
  const { readFile } = await import('node:fs/promises');
  const { createRequire } = await import('node:module');
  const { dirname } = await import('node:path');
  const localRequire = createRequire(__filenameForLoader());
  const path = localRequire.resolve('../guest/agw_node_guest.js');
  const source = await readFile(path, 'utf8');
  // Node's CJS loader wrapper is used only on locally built package content.
  // Calls are explicitly closed; guest-wide allocations die with this isolated
  // module. Disable generated finalizers so GC cannot re-enter a retired guest.
  const compiled = new Function('exports', 'require', 'module', '__filename', '__dirname', 'FinalizationRegistry', source);
  const local = { exports: {} };
  compiled(local.exports, createRequire(path), local, path, dirname(path), undefined);
  const binding = local.exports as Binding;
  const guest = new binding.Guest(JSON.stringify(options.selection ?? null), performance.now());
  return bindGuest(guest, options);
}
/** Internal host boundary, exported here for real-trap fault injection. */
export function bindGuest(guest: Guest, options: Options): Engine {
  const active = guest.active();
  const selectedTool = options.selection?.operation;
  const totalAllowance = options.work ?? 200_000_000;
  const allowance = options.selectiveRepair ? Math.floor(totalAllowance / 3) : totalAllowance;
  let retired = false;
  let draining = false;
  const publication = options.audit !== undefined;
  if (publication) guest.enable_audit?.(options.audit.tenant, randomUUID(), options.audit.capacity ?? 64, options.audit.maxBytes ?? 524288);
  const enter = <T>(work: () => T): T => {
    if (retired) throw Error('guest retired');
    try { return work(); } catch (error) { retired = true; throw error; }
  };
  const engine: Engine = {
    setReady(ready) { enter(() => guest.set_ready?.(ready)); },
    async drainAudit(sink, limit = 64) {
      if (!publication || retired || draining || guest.audit_peek === undefined || guest.audit_ack === undefined
        || guest.audit_stats === undefined || !Number.isSafeInteger(limit) || limit < 0 || limit > 256) return unavailableAudit;
      draining = true; let delivered = 0;
      try {
        for (let i = 0; i < limit && !retired; i++) {
          const json = enter(() => guest.audit_peek?.());
          if (json === undefined || json === 'null') break;
          if (Buffer.byteLength(json) > 131072) break;
          const artifact = validateAudit(JSON.parse(json));
          try { await sink(artifact); } catch { break; }
          if (retired || !enter(() => guest.audit_ack?.(artifact.id))) break;
          delivered++;
        }
        if (retired) return { ...unavailableAudit, delivered };
        const stats: unknown = JSON.parse(enter(() => guest.audit_stats?.()) ?? 'null');
        if (typeof stats !== 'object' || stats === null || !('pending' in stats) || !('dropped' in stats)
          || !Number.isSafeInteger(stats.pending) || Number(stats.pending) < 0
          || !Number.isSafeInteger(stats.dropped) || Number(stats.dropped) < 0) return { ...unavailableAudit, delivered };
        return { delivered, pending: Number(stats.pending), dropped: Number(stats.dropped), available: true };
      } catch { return { ...unavailableAudit, delivered }; }
      finally { draining = false; }
    },
    begin(value: unknown, mode: Mode, operation: Operation, failClosed: boolean, tool: string): EngineCall {
      if (retired) throw Error('guest retired');
      if (selectedTool !== undefined && selectedTool !== tool) throw Error('selection scope mismatch');
      const input = active ? captureBytes(value) : EMPTY_CAPTURE;
      return enter(() => {
        // A failed host capture exhausts this call's inspection allowance, so
        // the shared availability path also composes its bypass artifact.
        const call = guest.begin(input.bytes, input.truncated, mode, operation, performance.now(), failClosed, input.truncated ? 0 : allowance);
        if (publication) enter(() => call.audit_context?.(tool, new Date().toISOString()));
        let open = true;
        const close = (): void => { if (open) { open = false; if (!retired) enter(() => call.finish(EMPTY_CAPTURE.bytes, true, performance.now())); } };
        let request: Verdict;
        try { request = enter(() => verdict(call.request())); } catch (error) { close(); throw error; }
        return { request, close,
          finish(response: unknown) {
            const output = active ? captureBytes(response) : EMPTY_CAPTURE;
            return enter(() => {
              open = false; // wasm-bindgen consumes the Rust call even if it traps.
              const result = verdict(call.finish(output.bytes, output.truncated, performance.now()));
              return output.truncated ? { ...result, bypassed: true } : result;
            });
          },
        };
      });
    },
  };
  const repair = repairOptions(options.selectiveRepair, publication);
  if (!repair) return engine;
  if (!guest.prepare_rename) throw Error('selective repair guest unavailable');
  return selectiveEngine(engine, repair, (before, rule, mode) => enter(() => {
    if (!guest.prepare_rename) throw Error('selective repair guest unavailable');
    return guest.prepare_rename(new TextEncoder().encode(before), rule.from, rule.to, rule.confidence, mode, allowance);
  }));
}
// Replaced by the build script for each module format; no runtime discovery.
declare const __AGINDOW_FILENAME__: string;
function __filenameForLoader(): string { return __AGINDOW_FILENAME__; }
