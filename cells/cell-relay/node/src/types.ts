import type { SelectiveRepairOptions } from './selective.js';
export type { SelectiveRepairOptions, RenameRule, RepairCheck } from './selective.js';
import type { AuditDrain, AuditSink } from './audit.js';
export type Mode = 'observe' | 'repair' | 'strict';
export type Operation = 'read' | 'write' | 'unknown';
export type Callable = (...args: never[]) => unknown;
export interface StatisticalSelection {
  /** Explicit TI-19 materiality floor; no repair authority. */
  readonly materiality?: number;
  readonly tenant: string;
  readonly connection: string;
  readonly vendor: string;
  readonly operation: string;
  readonly segment: string;
  readonly fields: readonly {
    readonly side: 'request' | 'response';
    readonly path: string;
    /** Complete raw JSON documents from independently selected calls. */
    readonly baseline: readonly string[];
    readonly live: readonly string[];
    readonly enumeration: boolean;
  }[];
}
export interface Event {
  readonly type: 'bypass' | 'drift' | 'repair' | 'blocked' | 'error';
  readonly tool: string;
  readonly mode: Mode;
  readonly phase: 'request' | 'response';
  readonly reason?: string;
  readonly verdict?: Verdict;
  readonly side?: 'request' | 'response';
  readonly driftClass?: 'shape' | 'semantic' | 'unknown';
  readonly confidence?: number;
  readonly latencyUs: number;
  readonly auditId?: string;
  readonly byte_identical?: boolean;
}
export interface Config {
  readonly apiKey?: string;
  readonly endpoint?: string;
  readonly mode?: Mode;
  readonly failClosed?: boolean;
  readonly onEvent?: (event: Event) => void;
  readonly allowedTools?: readonly string[] | '*';
  readonly denyTools?: readonly string[];
  readonly sidecar?: string;
  readonly sync?: { readonly contracts?: number; readonly drift?: number };
  readonly buffer?: number;
  readonly spans?: boolean;
  readonly debug?: boolean;
  readonly headers?: Readonly<Record<string, string>>;
  readonly transformers?: Readonly<Record<string, Transformer>>;
}
export type Transformer = (value: unknown) => unknown;
export type AgindowConfig = Config;
export type AgindowEvent = Event;
export interface Options extends Config {
  readonly selectiveRepair?: SelectiveRepairOptions | false;
  readonly audit?: { readonly tenant: string; readonly capacity?: number; readonly maxBytes?: number };
  readonly selection?: StatisticalSelection;
  readonly work?: number;
}
export interface GuardOptions extends Config {
  readonly tool?: string;
  readonly toolName?: (...args: never[]) => string;
  readonly operation?: Operation;
  readonly inputShape?: 'raw' | 'parsed';
}
export interface PolicyFeedback {
  readonly error: 'drift_blocked' | 'drift_halted' | 'drift_passthrough';
  readonly tool: string;
  readonly drift_class: 'shape' | 'semantic' | 'unknown';
  readonly drift_id: string;
  readonly hint: string;
  readonly audit: string;
}
export interface Verdict {
  readonly halt: boolean;
  readonly bypassed: boolean;
  readonly class: 'shape' | 'semantic' | 'unknown' | null;
  readonly findings: number;
  readonly detectors: number;
  readonly malformed: boolean;
  readonly fault?: string | null;
  readonly detectorKinds?: readonly string[];
  readonly eligibleDetectors?: readonly string[];
  readonly suppressedDetectors?: readonly string[];
  readonly confidence?: number;
  readonly auditId?: string;
  readonly feedback?: PolicyFeedback;
  /** Trusted engine output only. Detector hypotheses never populate this. */
  readonly repair?: VerifiedRepair;
}
export interface VerifiedRepair {
  readonly verified: true;
  readonly value: unknown;
  readonly changes: readonly { readonly path: string; readonly kind: string; readonly to?: string }[];
}
export interface EngineCall {
  readonly request: Verdict;
  finish(value: unknown): Verdict;
  close(): void;
}
export interface Engine {
  /** Explicit host readiness control; does not load or refresh snapshots. */
  setReady?(ready: boolean): void;
  drainAudit?(sink: AuditSink, limit?: number): Promise<AuditDrain>;
  begin(value: unknown, mode: Mode, operation: Operation, failClosed: boolean, tool: string): EngineCall;
}
export type EngineLoader = (options: Options) => Promise<Engine>;
export class AgindowSetupError extends Error {
  override readonly name = 'AgindowSetupError';
}
export class AgindowDriftError extends Error {
  override readonly name = 'AgindowDriftError';
  readonly code: 'drift_blocked' | 'drift_halted' | 'contract_violation';
  readonly hint: string;
  readonly driftClass: PolicyFeedback['drift_class'];
  readonly driftId: string;
  readonly auditUrl: string;
  constructor(readonly tool: string, readonly mode: Mode, readonly phase: 'request' | 'response',
    readonly verdict: Verdict, readonly outcome: unknown, cause?: unknown) {
    super(`AGINDOW halted ${phase} for ${tool}`, { cause });
    this.code = mode === 'strict' ? 'drift_halted' : 'drift_blocked';
    this.driftClass = verdict.feedback?.drift_class ?? verdict.class ?? 'unknown';
    this.driftId = verdict.feedback?.drift_id ?? '';
    this.auditUrl = verdict.feedback?.audit ?? '';
    this.hint = verdict.feedback?.hint ?? (phase === 'response'
      ? 'Upstream completed. Inspect the retained outcome; do not blindly replay a write.'
      : 'Request was not dispatched. Review drift evidence before retrying.');
    if (phase === 'response' && !this.hint.includes('do not blindly replay')) {
      this.hint += ' Upstream completed. Inspect the retained outcome; do not blindly replay a write.';
    }
  }
}
