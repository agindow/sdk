import { guard } from './guard.js';
import type { Snapshot } from './guard.js';
import { AgindowDriftError } from './types.js';
import type { Engine, Event, GuardOptions } from './types.js';
import { simulateSandbox } from './sandbox.js';
import type { SandboxContract } from './sandbox.js';
export interface SimulationInput extends GuardOptions {
  readonly recorded: unknown;
  readonly args?: unknown;
  readonly partial?: boolean;
  readonly contract?: SandboxContract;
}
export interface Simulation {
  readonly status: 'no_drift' | 'observed' | 'blocked' | 'bypassed' | 'repaired' | 'degraded';
  readonly driftClass: 'shape' | 'semantic' | 'unknown' | null;
  readonly confidence: number | null;
  readonly changes: readonly unknown[];
  readonly byte_identical: boolean;
  readonly wouldWrite: boolean;
}
/** Execute only the same local phases as guard. The callable is a recording,
 * not an upstream operation. Simulation never loads credentials or a URL. */
export function simulate(tool: string, input: SimulationInput, config: Snapshot, engine: Engine | undefined): Simulation {
  const events: Event[] = [];
  let changes: readonly unknown[] = [];
  const result: Simulation = { status: 'bypassed', driftClass: null, confidence: null, changes: [],
    byte_identical: true, wouldWrite: (input.operation ?? 'read') !== 'read' };
  if (input.partial === true) return result;
  if (input.contract !== undefined) {
    if (input.contract.tool !== tool) return result;
    const contract = { ...input.contract, mode: input.mode ?? config.mode, operation: input.operation ?? input.contract.operation };
    const simulated = simulateSandbox(contract, input.args, input.recorded);
    return { ...simulated, status: simulated.status === 'ok' ? 'no_drift' : simulated.status };
  }
  const run = guard(() => input.recorded, { ...input, tool }, () => ({ ...config, ...input,
    mode: input.mode ?? config.mode, failClosed: input.failClosed ?? config.failClosed,
    spans: false, onEvent: event => { events.push(event); } }), () => engine === undefined ? undefined : {
      begin(...args) {
        const call = engine.begin(...args);
        const remember = (verdict: typeof call.request): typeof call.request => {
          if (verdict.repair !== undefined) changes = verdict.repair.changes;
          return verdict;
        };
        return { request: remember(call.request), finish: value => remember(call.finish(value)), close: () => call.close() };
      },
    },
    { request: () => input.args ?? {}, response: value => ({ inspect: true, value }) });
  try { run(); }
  catch (error) { if (!(error instanceof AgindowDriftError)) return result; }
  if (events.some(event => event.type === 'bypass')) return result;
  const repaired = events.find(event => event.type === 'repair');
  if (repaired !== undefined) return { ...result, status: 'repaired', changes,
    byte_identical: repaired.byte_identical ?? false, confidence: repaired.confidence ?? null, driftClass: repaired.driftClass ?? null };
  const drift = events.find(event => event.type === 'blocked') ?? events.find(event => event.type === 'drift');
  return { ...result, status: drift === undefined ? 'no_drift' : drift.type === 'blocked' ? 'blocked' : 'observed',
    driftClass: drift?.driftClass ?? null, confidence: drift?.confidence ?? null };
}
