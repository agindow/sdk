export type Json = null | boolean | number | string | Json[] | { [key: string]: Json };
export type Payload = { [key: string]: Json };
export type Category = 'runtime' | 'repo-and-pr';
export type Baseline = 'no-repair' | 'naive-full-payload';
export interface Finding { source: string; target: string; proposal: Json; confidence: number }
export interface AgentInput { id: string; payload: Payload; finding: Finding | null }
export interface Case extends AgentInput {
  class: string;
  scenario: string;
  provenance: 'synthetic';
  origin: 'authored-without-private-evidence';
  oracle: {
    expected: Payload;
    vendor: { target: string; type: 'number' | 'string'; allowed: Json[] | null };
    rationale: string;
  };
}
export interface Corpus { version: string; split: 'development' | 'heldout'; cases: Case[] }
export interface Result { id: string; write: Payload | null }
export interface Metrics {
  total: number; recovered: number; violations: number; blocked: number; rejected: number;
  dispatched: number; recoveryRate: number; violationRate: number; violationsPerDispatch: number | null;
}
export interface Report {
  protocol: 'conditional-safety-v1'; corpusVersion: string; corpusHash: string;
  split: 'development' | 'heldout'; category: Category; entry: string;
  metrics: Metrics; byClass: Record<string, Metrics>; byScenario: Record<string, Metrics>;
  results: Result[];
}
export interface Commitment { version: string; sha256: string }
export interface Attestation { validator: string; signature: string }
export interface Validator { id: string; publicKey: string }
